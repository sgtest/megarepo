/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.datastreams.lifecycle.action;

import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.IndicesRequest;
import org.elasticsearch.action.admin.indices.rollover.RolloverConfiguration;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.action.support.master.MasterNodeReadRequest;
import org.elasticsearch.common.collect.Iterators;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ChunkedToXContentObject;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xcontent.ToXContent;
import org.elasticsearch.xcontent.ToXContentObject;
import org.elasticsearch.xcontent.XContentBuilder;

import java.io.IOException;
import java.util.Arrays;
import java.util.Iterator;
import java.util.List;
import java.util.Objects;

/**
 * This action retrieves the data stream lifecycle from every data stream that has a data stream lifecycle configured.
 */
public class GetDataStreamLifecycleAction extends ActionType<GetDataStreamLifecycleAction.Response> {

    public static final GetDataStreamLifecycleAction INSTANCE = new GetDataStreamLifecycleAction();
    public static final String NAME = "indices:admin/data_stream/lifecycle/get";

    private GetDataStreamLifecycleAction() {
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
            this.includeDefaults = in.readBoolean();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeOptionalStringArray(names);
            indicesOptions.writeIndicesOptions(out);
            out.writeBoolean(includeDefaults);
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

    public static class Response extends ActionResponse implements ChunkedToXContentObject {
        public static final ParseField DATA_STREAMS_FIELD = new ParseField("data_streams");

        public record DataStreamLifecycle(String dataStreamName, @Nullable org.elasticsearch.cluster.metadata.DataStreamLifecycle lifecycle)
            implements
                Writeable,
                ToXContentObject {

            public static final ParseField NAME_FIELD = new ParseField("name");
            public static final ParseField LIFECYCLE_FIELD = new ParseField("lifecycle");

            DataStreamLifecycle(StreamInput in) throws IOException {
                this(in.readString(), in.readOptionalWriteable(org.elasticsearch.cluster.metadata.DataStreamLifecycle::new));
            }

            @Override
            public void writeTo(StreamOutput out) throws IOException {
                out.writeString(dataStreamName);
                out.writeOptionalWriteable(lifecycle);
            }

            @Override
            public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
                return toXContent(builder, params, null);
            }

            /**
             * Converts the response to XContent and passes the RolloverConditions, when provided, to the data stream lifecycle.
             */
            public XContentBuilder toXContent(XContentBuilder builder, Params params, @Nullable RolloverConfiguration rolloverConfiguration)
                throws IOException {
                builder.startObject();
                builder.field(NAME_FIELD.getPreferredName(), dataStreamName);
                if (lifecycle != null) {
                    builder.field(LIFECYCLE_FIELD.getPreferredName());
                    lifecycle.toXContent(builder, params, rolloverConfiguration);
                }
                builder.endObject();
                return builder;
            }
        }

        private final List<DataStreamLifecycle> dataStreamLifecycles;
        @Nullable
        private final RolloverConfiguration rolloverConfiguration;

        public Response(List<DataStreamLifecycle> dataStreamLifecycles) {
            this(dataStreamLifecycles, null);
        }

        public Response(List<DataStreamLifecycle> dataStreamLifecycles, @Nullable RolloverConfiguration rolloverConfiguration) {
            this.dataStreamLifecycles = dataStreamLifecycles;
            this.rolloverConfiguration = rolloverConfiguration;
        }

        public Response(StreamInput in) throws IOException {
            this(in.readCollectionAsList(Response.DataStreamLifecycle::new), in.readOptionalWriteable(RolloverConfiguration::new));
        }

        public List<DataStreamLifecycle> getDataStreamLifecycles() {
            return dataStreamLifecycles;
        }

        @Nullable
        public RolloverConfiguration getRolloverConfiguration() {
            return rolloverConfiguration;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            out.writeCollection(dataStreamLifecycles);
            out.writeOptionalWriteable(rolloverConfiguration);
        }

        @Override
        public Iterator<ToXContent> toXContentChunked(ToXContent.Params outerParams) {
            return Iterators.concat(Iterators.single((builder, params) -> {
                builder.startObject();
                builder.startArray(DATA_STREAMS_FIELD.getPreferredName());
                return builder;
            }),
                Iterators.map(
                    dataStreamLifecycles.iterator(),
                    dataStreamLifecycle -> (builder, params) -> dataStreamLifecycle.toXContent(builder, params, rolloverConfiguration)
                ),
                Iterators.single((builder, params) -> {
                    builder.endArray();
                    builder.endObject();
                    return builder;
                })
            );
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Response response = (Response) o;
            return dataStreamLifecycles.equals(response.dataStreamLifecycles)
                && Objects.equals(rolloverConfiguration, response.rolloverConfiguration);
        }

        @Override
        public int hashCode() {
            return Objects.hash(dataStreamLifecycles, rolloverConfiguration);
        }
    }
}
