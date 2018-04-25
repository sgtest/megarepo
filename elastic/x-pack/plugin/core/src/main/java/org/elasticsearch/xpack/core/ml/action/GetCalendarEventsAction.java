/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.action;

import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestBuilder;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ValidateActions;
import org.elasticsearch.client.ElasticsearchClient;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ml.action.util.PageParams;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.calendars.Calendar;
import org.elasticsearch.xpack.core.ml.calendars.ScheduledEvent;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.IOException;
import java.util.Objects;

public class GetCalendarEventsAction extends Action<GetCalendarEventsAction.Request, GetCalendarEventsAction.Response,
        GetCalendarEventsAction.RequestBuilder> {
    public static final GetCalendarEventsAction INSTANCE = new GetCalendarEventsAction();
    public static final String NAME = "cluster:monitor/xpack/ml/calendars/events/get";

    private GetCalendarEventsAction() {
        super(NAME);
    }

    @Override
    public RequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new RequestBuilder(client);
    }

    @Override
    public Response newResponse() {
        return new Response();
    }

    public static class Request extends ActionRequest implements ToXContentObject {

        public static final ParseField START = new ParseField("start");
        public static final ParseField END = new ParseField("end");

        private static final ObjectParser<Request, Void> PARSER = new ObjectParser<>(NAME, Request::new);

        static {
            PARSER.declareString(Request::setCalendarId, Calendar.ID);
            PARSER.declareString(Request::setStart, START);
            PARSER.declareString(Request::setEnd, END);
            PARSER.declareString(Request::setJobId, Job.ID);
            PARSER.declareObject(Request::setPageParams, PageParams.PARSER, PageParams.PAGE);
        }

        public static Request parseRequest(String calendarId, XContentParser parser) {
            Request request = PARSER.apply(parser, null);
            if (calendarId != null) {
                request.setCalendarId(calendarId);
            }
            return request;
        }

        private String calendarId;
        private String start;
        private String end;
        private String jobId;
        private PageParams pageParams = PageParams.defaultParams();

        public Request() {
        }

        public Request(String calendarId) {
            setCalendarId(calendarId);
        }

        public String getCalendarId() {
            return calendarId;
        }

        private void setCalendarId(String calendarId) {
            this.calendarId = ExceptionsHelper.requireNonNull(calendarId, Calendar.ID.getPreferredName());
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

        public String getJobId() {
            return jobId;
        }

        public void setJobId(String jobId) {
            this.jobId = jobId;
        }

        public PageParams getPageParams() {
            return pageParams;
        }

        public void setPageParams(PageParams pageParams) {
            this.pageParams = Objects.requireNonNull(pageParams);
        }

        @Override
        public ActionRequestValidationException validate() {
            ActionRequestValidationException e = null;

            boolean calendarIdIsAll = GetCalendarsAction.Request.ALL.equals(calendarId);
            if (jobId != null && calendarIdIsAll == false) {
                e = ValidateActions.addValidationError("If " + Job.ID.getPreferredName() + " is used " +
                        Calendar.ID.getPreferredName() + " must be '" + GetCalendarsAction.Request.ALL + "'", e);
            }
            return e;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            calendarId = in.readString();
            start = in.readOptionalString();
            end = in.readOptionalString();
            jobId = in.readOptionalString();
            pageParams = new PageParams(in);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(calendarId);
            out.writeOptionalString(start);
            out.writeOptionalString(end);
            out.writeOptionalString(jobId);
            pageParams.writeTo(out);
        }

        @Override
        public int hashCode() {
            return Objects.hash(calendarId, start, end, pageParams, jobId);
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
            return Objects.equals(calendarId, other.calendarId) && Objects.equals(start, other.start)
                    && Objects.equals(end, other.end) && Objects.equals(pageParams, other.pageParams)
                    && Objects.equals(jobId, other.jobId);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(Calendar.ID.getPreferredName(), calendarId);
            if (start != null) {
                builder.field(START.getPreferredName(), start);
            }
            if (end != null) {
                builder.field(END.getPreferredName(), end);
            }
            if (jobId != null) {
                builder.field(Job.ID.getPreferredName(), jobId);
            }
            builder.field(PageParams.PAGE.getPreferredName(), pageParams);
            builder.endObject();
            return builder;
        }
    }

    public static class RequestBuilder extends ActionRequestBuilder<Request, Response, RequestBuilder> {

        public RequestBuilder(ElasticsearchClient client) {
            super(client, INSTANCE, new Request());
        }
    }

    public static class Response extends ActionResponse implements ToXContentObject {

        private QueryPage<ScheduledEvent> scheduledEvents;

        public Response() {
        }

        public Response(QueryPage<ScheduledEvent> scheduledEvents) {
            this.scheduledEvents = scheduledEvents;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            scheduledEvents = new QueryPage<>(in, ScheduledEvent::new);

        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            scheduledEvents.writeTo(out);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            return scheduledEvents.toXContent(builder, params);
        }

        @Override
        public int hashCode() {
            return Objects.hash(scheduledEvents);
        }

        @Override
        public boolean equals(Object obj) {
            if (obj == null) {
                return false;
            }
            if (getClass() != obj.getClass()) {
                return false;
            }
            Response other = (Response) obj;
            return Objects.equals(scheduledEvents, other.scheduledEvents);
        }
    }

}
