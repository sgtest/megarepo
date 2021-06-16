/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.support.tasks.BaseTasksRequest;
import org.elasticsearch.action.support.tasks.BaseTasksResponse;
import org.elasticsearch.common.collect.MapBuilder;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.xpack.core.ml.MlTasks;

import java.io.IOException;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.stream.Collectors;


/**
 * Internal only action to get the current running state of a datafeed
 */
public class GetDatafeedRunningStateAction extends ActionType<GetDatafeedRunningStateAction.Response> {

    public static final GetDatafeedRunningStateAction INSTANCE = new GetDatafeedRunningStateAction();
    public static final String NAME = "cluster:internal/xpack/ml/datafeed/running_state";

    private GetDatafeedRunningStateAction() {
        super(NAME, GetDatafeedRunningStateAction.Response::new);
    }

    public static class Request extends BaseTasksRequest<Request> {

        private final Set<String> datafeedTaskIds;

        public Request(List<String> datafeedIds) {
            this.datafeedTaskIds = datafeedIds.stream().map(MlTasks::datafeedTaskId).collect(Collectors.toSet());
        }

        public Request(StreamInput in) throws IOException {
            super(in);
            this.datafeedTaskIds = in.readSet(StreamInput::readString);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeStringCollection(datafeedTaskIds);
        }

        public Set<String> getDatafeedTaskIds() {
            return datafeedTaskIds;
        }

        @Override
        public boolean match(Task task) {
            return task instanceof StartDatafeedAction.DatafeedTaskMatcher && datafeedTaskIds.contains(task.getDescription());
        }
    }

    public static class Response extends BaseTasksResponse {

        public static class RunningState implements Writeable, ToXContentObject {

            // Is the datafeed a "realtime" datafeed, meaning it was started without an end_time
            private final boolean realTimeConfigured;
            // Has the look back finished and are we now running on "real-time" data
            private final boolean realTimeRunning;

            public RunningState(boolean realTimeConfigured, boolean realTimeRunning) {
                this.realTimeConfigured = realTimeConfigured;
                this.realTimeRunning = realTimeRunning;
            }

            public RunningState(StreamInput in) throws IOException {
                this.realTimeConfigured = in.readBoolean();
                this.realTimeRunning = in.readBoolean();
            }

            @Override
            public boolean equals(Object o) {
                if (this == o) return true;
                if (o == null || getClass() != o.getClass()) return false;
                RunningState that = (RunningState) o;
                return realTimeConfigured == that.realTimeConfigured && realTimeRunning == that.realTimeRunning;
            }

            @Override
            public int hashCode() {
                return Objects.hash(realTimeConfigured, realTimeRunning);
            }

            @Override
            public void writeTo(StreamOutput out) throws IOException {
                out.writeBoolean(realTimeConfigured);
                out.writeBoolean(realTimeRunning);
            }

            @Override
            public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
                builder.startObject();
                builder.field("real_time_configured", realTimeConfigured);
                builder.field("real_time_running", realTimeRunning);
                builder.endObject();
                return builder;
            }
        }

        private final Map<String, RunningState> datafeedRunningState;

        public static Response fromResponses(List<Response> responses) {
            return new Response(responses.stream()
                .flatMap(r -> r.datafeedRunningState.entrySet().stream())
                .filter(entry -> entry.getValue() != null)
                .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue)));
        }

        public static Response fromTaskAndState(String datafeedId, RunningState runningState) {
            return new Response(MapBuilder.<String, RunningState>newMapBuilder().put(datafeedId, runningState).map());
        }

        public Response(StreamInput in) throws IOException {
            super(in);
            datafeedRunningState = in.readMap(StreamInput::readString, RunningState::new);
        }

        public Response(Map<String, RunningState> runtimeStateMap) {
            super(null, null);
            this.datafeedRunningState = runtimeStateMap;
        }

        public Optional<RunningState> getRunningState(String datafeedId) {
            return Optional.ofNullable(datafeedRunningState.get(datafeedId));
        }

        public Map<String, RunningState> getDatafeedRunningState() {
            return datafeedRunningState;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeMap(datafeedRunningState, StreamOutput::writeString, (o, w) -> w.writeTo(o));
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Response response = (Response) o;
            return Objects.equals(this.datafeedRunningState, response.datafeedRunningState);
        }

        @Override
        public int hashCode() {
            return Objects.hash(datafeedRunningState);
        }
    }

}
