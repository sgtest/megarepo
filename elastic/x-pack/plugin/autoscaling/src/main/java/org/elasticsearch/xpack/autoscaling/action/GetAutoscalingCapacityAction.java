/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.autoscaling.action;

import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.support.master.AcknowledgedRequest;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.autoscaling.capacity.AutoscalingDeciderResults;

import java.io.IOException;
import java.util.Map;
import java.util.Objects;
import java.util.SortedMap;
import java.util.TreeMap;

public class GetAutoscalingCapacityAction extends ActionType<GetAutoscalingCapacityAction.Response> {

    public static final GetAutoscalingCapacityAction INSTANCE = new GetAutoscalingCapacityAction();
    public static final String NAME = "cluster:admin/autoscaling/get_autoscaling_capacity";

    private GetAutoscalingCapacityAction() {
        super(NAME, Response::new);
    }

    public static class Request extends AcknowledgedRequest<GetAutoscalingCapacityAction.Request> {

        public Request() {

        }

        public Request(final StreamInput in) throws IOException {
            super(in);
        }

        @Override
        public void writeTo(final StreamOutput out) throws IOException {
            super.writeTo(out);
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public boolean equals(final Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            return true;
        }

        @Override
        public int hashCode() {
            return 0;
        }

    }

    public static class Response extends ActionResponse implements ToXContentObject {

        private final SortedMap<String, AutoscalingDeciderResults> results;

        public Response(final SortedMap<String, AutoscalingDeciderResults> results) {
            this.results = Objects.requireNonNull(results);
        }

        public Response(final StreamInput in) throws IOException {
            super(in);
            results = new TreeMap<>(in.readMap(StreamInput::readString, AutoscalingDeciderResults::new));
        }

        @Override
        public void writeTo(final StreamOutput out) throws IOException {
            out.writeMap(results, StreamOutput::writeString, (o, decision) -> decision.writeTo(o));
        }

        public SortedMap<String, AutoscalingDeciderResults> results() {
            return results;
        }

        @Override
        public XContentBuilder toXContent(final XContentBuilder builder, final Params params) throws IOException {
            builder.startObject();
            {
                builder.startObject("policies");
                {
                    for (Map.Entry<String, AutoscalingDeciderResults> entry : results.entrySet()) {
                        builder.field(entry.getKey(), entry.getValue());
                    }
                }
                builder.endObject();
            }
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(final Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            final Response response = (Response) o;
            return results.equals(response.results);
        }

        @Override
        public int hashCode() {
            return Objects.hash(results);
        }

        public Map<String, AutoscalingDeciderResults> getResults() {
            return results;
        }

        @Override
        public String toString() {
            return Strings.toString(this);
        }
    }

}
