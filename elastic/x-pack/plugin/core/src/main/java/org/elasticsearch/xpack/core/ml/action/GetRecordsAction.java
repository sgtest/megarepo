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
import org.elasticsearch.client.ElasticsearchClient;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ml.action.util.PageParams;
import org.elasticsearch.xpack.core.ml.action.util.QueryPage;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.results.AnomalyRecord;
import org.elasticsearch.xpack.core.ml.job.results.Influencer;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.IOException;
import java.util.Objects;

public class GetRecordsAction extends Action<GetRecordsAction.Request, GetRecordsAction.Response, GetRecordsAction.RequestBuilder> {

    public static final GetRecordsAction INSTANCE = new GetRecordsAction();
    public static final String NAME = "cluster:monitor/xpack/ml/job/results/records/get";

    private GetRecordsAction() {
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
        public static final ParseField EXCLUDE_INTERIM = new ParseField("exclude_interim");
        public static final ParseField RECORD_SCORE_FILTER = new ParseField("record_score");
        public static final ParseField SORT = new ParseField("sort");
        public static final ParseField DESCENDING = new ParseField("desc");

        private static final ObjectParser<Request, Void> PARSER = new ObjectParser<>(NAME, Request::new);

        static {
            PARSER.declareString((request, jobId) -> request.jobId = jobId, Job.ID);
            PARSER.declareStringOrNull(Request::setStart, START);
            PARSER.declareStringOrNull(Request::setEnd, END);
            PARSER.declareString(Request::setSort, SORT);
            PARSER.declareBoolean(Request::setDescending, DESCENDING);
            PARSER.declareBoolean(Request::setExcludeInterim, EXCLUDE_INTERIM);
            PARSER.declareObject(Request::setPageParams, PageParams.PARSER, PageParams.PAGE);
            PARSER.declareDouble(Request::setRecordScore, RECORD_SCORE_FILTER);
        }

        public static Request parseRequest(String jobId, XContentParser parser) {
            Request request = PARSER.apply(parser, null);
            if (jobId != null) {
                request.jobId = jobId;
            }
            return request;
        }

        private String jobId;
        private String start;
        private String end;
        private boolean excludeInterim = false;
        private PageParams pageParams = new PageParams();
        private double recordScoreFilter = 0.0;
        private String sort = Influencer.INFLUENCER_SCORE.getPreferredName();
        private boolean descending = true;

        public Request() {
        }

        public Request(String jobId) {
            this.jobId = ExceptionsHelper.requireNonNull(jobId, Job.ID.getPreferredName());
        }

        public String getJobId() {
            return jobId;
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

        public boolean isDescending() {
            return descending;
        }

        public void setDescending(boolean descending) {
            this.descending = descending;
        }

        public boolean isExcludeInterim() {
            return excludeInterim;
        }

        public void setExcludeInterim(boolean excludeInterim) {
            this.excludeInterim = excludeInterim;
        }

        public void setPageParams(PageParams pageParams) {
            this.pageParams = pageParams;
        }
        public PageParams getPageParams() {
            return pageParams;
        }

        public double getRecordScoreFilter() {
            return recordScoreFilter;
        }

        public void setRecordScore(double recordScoreFilter) {
            this.recordScoreFilter = recordScoreFilter;
        }

        public String getSort() {
            return sort;
        }

        public void setSort(String sort) {
            this.sort = ExceptionsHelper.requireNonNull(sort, SORT.getPreferredName());
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            jobId = in.readString();
            excludeInterim = in.readBoolean();
            pageParams = new PageParams(in);
            start = in.readOptionalString();
            end = in.readOptionalString();
            sort = in.readOptionalString();
            descending = in.readBoolean();
            recordScoreFilter = in.readDouble();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(jobId);
            out.writeBoolean(excludeInterim);
            pageParams.writeTo(out);
            out.writeOptionalString(start);
            out.writeOptionalString(end);
            out.writeOptionalString(sort);
            out.writeBoolean(descending);
            out.writeDouble(recordScoreFilter);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(Job.ID.getPreferredName(), jobId);
            builder.field(START.getPreferredName(), start);
            builder.field(END.getPreferredName(), end);
            builder.field(SORT.getPreferredName(), sort);
            builder.field(DESCENDING.getPreferredName(), descending);
            builder.field(RECORD_SCORE_FILTER.getPreferredName(), recordScoreFilter);
            builder.field(EXCLUDE_INTERIM.getPreferredName(), excludeInterim);
            builder.field(PageParams.PAGE.getPreferredName(), pageParams);
            builder.endObject();
            return builder;
        }

        @Override
        public int hashCode() {
            return Objects.hash(jobId, start, end, sort, descending, recordScoreFilter, excludeInterim, pageParams);
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
            return Objects.equals(jobId, other.jobId) &&
                    Objects.equals(start, other.start) &&
                    Objects.equals(end, other.end) &&
                    Objects.equals(sort, other.sort) &&
                    Objects.equals(descending, other.descending) &&
                    Objects.equals(recordScoreFilter, other.recordScoreFilter) &&
                    Objects.equals(excludeInterim, other.excludeInterim) &&
                    Objects.equals(pageParams, other.pageParams);
        }
    }

    static class RequestBuilder extends ActionRequestBuilder<Request, Response, RequestBuilder> {

        RequestBuilder(ElasticsearchClient client) {
            super(client, INSTANCE, new Request());
        }
    }

    public static class Response extends ActionResponse implements ToXContentObject {

        private QueryPage<AnomalyRecord> records;

        public Response() {
        }

        public Response(QueryPage<AnomalyRecord> records) {
            this.records = records;
        }

        public QueryPage<AnomalyRecord> getRecords() {
            return records;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            records = new QueryPage<>(in, AnomalyRecord::new);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            records.writeTo(out);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            records.doXContentBody(builder, params);
            builder.endObject();
            return builder;
        }

        @Override
        public int hashCode() {
            return Objects.hash(records);
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
            return Objects.equals(records, other.records);
        }

        @Override
        public final String toString() {
            return Strings.toString(this);
        }
    }

}
