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
import org.elasticsearch.common.xcontent.StatusToXContentObject;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshotField;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.IOException;
import java.util.Objects;

public class UpdateModelSnapshotAction extends Action<UpdateModelSnapshotAction.Request,
        UpdateModelSnapshotAction.Response, UpdateModelSnapshotAction.RequestBuilder> {

    public static final UpdateModelSnapshotAction INSTANCE = new UpdateModelSnapshotAction();
    public static final String NAME = "cluster:admin/xpack/ml/job/model_snapshots/update";

    private UpdateModelSnapshotAction() {
        super(NAME);
    }

    @Override
    public UpdateModelSnapshotAction.RequestBuilder newRequestBuilder(ElasticsearchClient client) {
        return new RequestBuilder(client, this);
    }

    @Override
    public UpdateModelSnapshotAction.Response newResponse() {
        return new Response();
    }

    public static class Request extends ActionRequest implements ToXContentObject {

        private static final ObjectParser<Request, Void> PARSER = new ObjectParser<>(NAME, Request::new);

        static {
            PARSER.declareString((request, jobId) -> request.jobId = jobId, Job.ID);
            PARSER.declareString((request, snapshotId) -> request.snapshotId = snapshotId, ModelSnapshotField.SNAPSHOT_ID);
            PARSER.declareString(Request::setDescription, ModelSnapshot.DESCRIPTION);
            PARSER.declareBoolean(Request::setRetain, ModelSnapshot.RETAIN);
        }

        public static Request parseRequest(String jobId, String snapshotId, XContentParser parser) {
            Request request = PARSER.apply(parser, null);
            if (jobId != null) {
                request.jobId = jobId;
            }
            if (snapshotId != null) {
                request.snapshotId = snapshotId;
            }
            return request;
        }

        private String jobId;
        private String snapshotId;
        private String description;
        private Boolean retain;

        public Request() {
        }

        public Request(String jobId, String snapshotId) {
            this.jobId = ExceptionsHelper.requireNonNull(jobId, Job.ID.getPreferredName());
            this.snapshotId = ExceptionsHelper.requireNonNull(snapshotId, ModelSnapshotField.SNAPSHOT_ID.getPreferredName());
        }

        public String getJobId() {
            return jobId;
        }

        public String getSnapshotId() {
            return snapshotId;
        }

        public String getDescription() {
            return description;
        }

        public void setDescription(String description) {
            this.description = description;
        }

        public Boolean getRetain() {
            return retain;
        }

        public void setRetain(Boolean retain) {
            this.retain = retain;
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            jobId = in.readString();
            snapshotId = in.readString();
            description = in.readOptionalString();
            retain = in.readOptionalBoolean();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(jobId);
            out.writeString(snapshotId);
            out.writeOptionalString(description);
            out.writeOptionalBoolean(retain);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(Job.ID.getPreferredName(), jobId);
            builder.field(ModelSnapshotField.SNAPSHOT_ID.getPreferredName(), snapshotId);
            if (description != null) {
                builder.field(ModelSnapshot.DESCRIPTION.getPreferredName(), description);
            }
            if (retain != null) {
                builder.field(ModelSnapshot.RETAIN.getPreferredName(), retain);
            }
            builder.endObject();
            return builder;
        }

        @Override
        public int hashCode() {
            return Objects.hash(jobId, snapshotId, description, retain);
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
            return Objects.equals(jobId, other.jobId)
                    && Objects.equals(snapshotId, other.snapshotId)
                    && Objects.equals(description, other.description)
                    && Objects.equals(retain, other.retain);
        }
    }

    public static class Response extends ActionResponse implements StatusToXContentObject {

        private static final ParseField ACKNOWLEDGED = new ParseField("acknowledged");
        private static final ParseField MODEL = new ParseField("model");

        private ModelSnapshot model;

        public Response() {

        }

        public Response(ModelSnapshot modelSnapshot) {
            model = modelSnapshot;
        }

        public ModelSnapshot getModel() {
            return model;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            model = new ModelSnapshot(in);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            model.writeTo(out);
        }

        @Override
        public RestStatus status() {
            return RestStatus.OK;
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(ACKNOWLEDGED.getPreferredName(), true);
            builder.field(MODEL.getPreferredName());
            builder = model.toXContent(builder, params);
            builder.endObject();
            return builder;
        }

        @Override
        public int hashCode() {
            return Objects.hash(model);
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
            return Objects.equals(model, other.model);
        }

        @Override
        public final String toString() {
            return Strings.toString(this);
        }
    }

    public static class RequestBuilder extends ActionRequestBuilder<Request, Response, RequestBuilder> {

        public RequestBuilder(ElasticsearchClient client, UpdateModelSnapshotAction action) {
            super(client, action, new Request());
        }
    }

}
