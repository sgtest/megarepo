/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.rollup.action;

import org.elasticsearch.Version;
import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionRequestBuilder;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.support.tasks.BaseTasksRequest;
import org.elasticsearch.action.support.tasks.BaseTasksResponse;
import org.elasticsearch.client.ElasticsearchClient;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.core.rollup.RollupField;

import java.io.IOException;
import java.util.Collections;
import java.util.Objects;
import java.util.concurrent.TimeUnit;

public class StopRollupJobAction extends Action<StopRollupJobAction.Response> {

    public static final StopRollupJobAction INSTANCE = new StopRollupJobAction();
    public static final String NAME = "cluster:admin/xpack/rollup/stop";
    public static final ParseField WAIT_FOR_COMPLETION = new ParseField("wait_for_completion");
    public static final ParseField TIMEOUT = new ParseField("timeout");
    public static final TimeValue DEFAULT_TIMEOUT = new TimeValue(30, TimeUnit.SECONDS);

    private StopRollupJobAction() {
        super(NAME);
    }

    @Override
    public Response newResponse() {
        return new Response();
    }

    public static class Request extends BaseTasksRequest<Request> implements ToXContent {
        private String id;
        private boolean waitForCompletion = false;
        private TimeValue timeout = null;

        public Request (String id) {
            this(id, false, null);
        }

        public Request(String id, boolean waitForCompletion, @Nullable TimeValue timeout) {
            this.id = ExceptionsHelper.requireNonNull(id, RollupField.ID.getPreferredName());
            this.timeout = timeout == null ? DEFAULT_TIMEOUT : timeout;
            this.waitForCompletion = waitForCompletion;
        }

        public Request() {}

        public String getId() {
            return id;
        }

        public TimeValue timeout() {
            return timeout;
        }

        public boolean waitForCompletion() {
            return waitForCompletion;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            id = in.readString();
            if (in.getVersion().onOrAfter(Version.V_6_6_0)) {
                waitForCompletion = in.readBoolean();
                timeout = in.readTimeValue();
            }
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(id);
            if (out.getVersion().onOrAfter(Version.V_6_6_0)) {
                out.writeBoolean(waitForCompletion);
                out.writeTimeValue(timeout);
            }
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.field(RollupField.ID.getPreferredName(), id);
            builder.field(WAIT_FOR_COMPLETION.getPreferredName(), waitForCompletion);
            if (timeout != null) {
                builder.field(TIMEOUT.getPreferredName(), timeout);
            }
            return builder;
        }

        @Override
        public int hashCode() {
            return Objects.hash(id, waitForCompletion, timeout);
        }

        @Override
        public boolean equals(Object obj) {
            if (obj == null) {
                return false;
            }
            if (getClass() != obj.getClass()) {
                return false;
            }
            Request other = (Request) obj;
            return Objects.equals(id, other.id)
                && Objects.equals(waitForCompletion, other.waitForCompletion)
                && Objects.equals(timeout, other.timeout);
        }
    }

    public static class RequestBuilder extends ActionRequestBuilder<Request, Response> {

        protected RequestBuilder(ElasticsearchClient client, StopRollupJobAction action) {
            super(client, action, new Request());
        }
    }

    public static class Response extends BaseTasksResponse implements Writeable, ToXContentObject {

        private boolean stopped;

        public Response() {
            super(Collections.emptyList(), Collections.emptyList());
        }

        public Response(StreamInput in) throws IOException {
            super(Collections.emptyList(), Collections.emptyList());
            readFrom(in);
        }

        public Response(boolean stopped) {
            super(Collections.emptyList(), Collections.emptyList());
            this.stopped = stopped;
        }

        public boolean isStopped() {
            return stopped;
        }


        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            stopped = in.readBoolean();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeBoolean(stopped);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field("stopped", stopped);
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Response response = (Response) o;
            return stopped == response.stopped;
        }

        @Override
        public int hashCode() {
            return Objects.hash(stopped);
        }
    }
}
