/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.Version;
import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionRequestBuilder;
import org.elasticsearch.action.support.tasks.BaseTasksResponse;
import org.elasticsearch.client.ElasticsearchClient;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.output.FlushAcknowledgement;

import java.io.IOException;
import java.util.Date;
import java.util.Objects;

public class FlushJobAction extends Action<FlushJobAction.Request, FlushJobAction.Response, FlushJobAction.RequestBuilder> {

    public static final FlushJobAction INSTANCE = new FlushJobAction();
    public static final String NAME = "cluster:admin/xpack/ml/job/flush";

    private FlushJobAction() {
        super(NAME);
    }

    @Override
    public RequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new RequestBuilder(client, this);
    }

    @Override
    public Response newResponse() {
        return new Response();
    }

    public static class Request extends JobTaskRequest<Request> implements ToXContentObject {

        public static final ParseField CALC_INTERIM = new ParseField("calc_interim");
        public static final ParseField START = new ParseField("start");
        public static final ParseField END = new ParseField("end");
        public static final ParseField ADVANCE_TIME = new ParseField("advance_time");
        public static final ParseField SKIP_TIME = new ParseField("skip_time");

        private static final ObjectParser<Request, Void> PARSER = new ObjectParser<>(NAME, Request::new);

        static {
            PARSER.declareString((request, jobId) -> request.jobId = jobId, Job.ID);
            PARSER.declareBoolean(Request::setCalcInterim, CALC_INTERIM);
            PARSER.declareString(Request::setStart, START);
            PARSER.declareString(Request::setEnd, END);
            PARSER.declareString(Request::setAdvanceTime, ADVANCE_TIME);
            PARSER.declareString(Request::setSkipTime, SKIP_TIME);
        }

        public static Request parseRequest(String jobId, XContentParser parser) {
            Request request = PARSER.apply(parser, null);
            if (jobId != null) {
                request.jobId = jobId;
            }
            return request;
        }

        private boolean calcInterim = false;
        private String start;
        private String end;
        private String advanceTime;
        private String skipTime;

        public Request() {
        }

        public Request(String jobId) {
            super(jobId);
        }

        public boolean getCalcInterim() {
            return calcInterim;
        }

        public void setCalcInterim(boolean calcInterim) {
            this.calcInterim = calcInterim;
        }

        public String getStart() {
            return start;
        }

        public void setStart(String start) {
            this.start = start;
        }

        public String getEnd() {
            return end;
        }

        public void setEnd(String end) {
            this.end = end;
        }

        public String getAdvanceTime() {
            return advanceTime;
        }

        public void setAdvanceTime(String advanceTime) {
            this.advanceTime = advanceTime;
        }

        public String getSkipTime() {
            return skipTime;
        }

        public void setSkipTime(String skipTime) {
            this.skipTime = skipTime;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            calcInterim = in.readBoolean();
            start = in.readOptionalString();
            end = in.readOptionalString();
            advanceTime = in.readOptionalString();
            if (in.getVersion().after(Version.V_5_5_0)) {
                skipTime = in.readOptionalString();
            }
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeBoolean(calcInterim);
            out.writeOptionalString(start);
            out.writeOptionalString(end);
            out.writeOptionalString(advanceTime);
            if (out.getVersion().after(Version.V_5_5_0)) {
                out.writeOptionalString(skipTime);
            }
        }

        @Override
        public int hashCode() {
            return Objects.hash(jobId, calcInterim, start, end, advanceTime, skipTime);
        }

        @Override
        public boolean equals(Object obj) {
            if (this == obj) {
                return true;
            }
            if (obj == null || getClass() != obj.getClass()) {
                return false;
            }
            Request other = (Request) obj;
            return Objects.equals(jobId, other.jobId) &&
                    calcInterim == other.calcInterim &&
                    Objects.equals(start, other.start) &&
                    Objects.equals(end, other.end) &&
                    Objects.equals(advanceTime, other.advanceTime) &&
                    Objects.equals(skipTime, other.skipTime);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(Job.ID.getPreferredName(), jobId);
            builder.field(CALC_INTERIM.getPreferredName(), calcInterim);
            if (start != null) {
                builder.field(START.getPreferredName(), start);
            }
            if (end != null) {
                builder.field(END.getPreferredName(), end);
            }
            if (advanceTime != null) {
                builder.field(ADVANCE_TIME.getPreferredName(), advanceTime);
            }
            if (skipTime != null) {
                builder.field(SKIP_TIME.getPreferredName(), skipTime);
            }
            builder.endObject();
            return builder;
        }
    }

    static class RequestBuilder extends ActionRequestBuilder<Request, Response, RequestBuilder> {

        RequestBuilder(ElasticsearchClient client, FlushJobAction action) {
            super(client, action, new Request());
        }
    }

    public static class Response extends BaseTasksResponse implements Writeable, ToXContentObject {

        private boolean flushed;
        private Date lastFinalizedBucketEnd;

        public Response() {
            super(null, null);
        }

        public Response(boolean flushed, @Nullable Date lastFinalizedBucketEnd) {
            super(null, null);
            this.flushed = flushed;
            this.lastFinalizedBucketEnd = lastFinalizedBucketEnd;
        }

        public boolean isFlushed() {
            return flushed;
        }

        public Date getLastFinalizedBucketEnd() {
            return lastFinalizedBucketEnd;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            flushed = in.readBoolean();
            if (in.getVersion().after(Version.V_5_5_0)) {
                lastFinalizedBucketEnd = new Date(in.readVLong());
            }
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeBoolean(flushed);
            if (out.getVersion().after(Version.V_5_5_0)) {
                out.writeVLong(lastFinalizedBucketEnd.getTime());
            }
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field("flushed", flushed);
            if (lastFinalizedBucketEnd != null) {
                builder.timeField(FlushAcknowledgement.LAST_FINALIZED_BUCKET_END.getPreferredName(),
                        FlushAcknowledgement.LAST_FINALIZED_BUCKET_END.getPreferredName() + "_string", lastFinalizedBucketEnd.getTime());
            }
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Response response = (Response) o;
            return flushed == response.flushed &&
                    Objects.equals(lastFinalizedBucketEnd, response.lastFinalizedBucketEnd);
        }

        @Override
        public int hashCode() {
            return Objects.hash(flushed, lastFinalizedBucketEnd);
        }
    }

}


