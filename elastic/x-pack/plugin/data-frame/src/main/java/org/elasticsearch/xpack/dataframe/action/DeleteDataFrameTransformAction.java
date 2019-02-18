/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.dataframe.action;

import org.elasticsearch.action.Action;
import org.elasticsearch.action.ActionRequestBuilder;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.FailedNodeException;
import org.elasticsearch.action.TaskOperationFailure;
import org.elasticsearch.action.support.tasks.BaseTasksRequest;
import org.elasticsearch.action.support.tasks.BaseTasksResponse;
import org.elasticsearch.client.ElasticsearchClient;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.xcontent.ToXContentFragment;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.xpack.core.dataframe.DataFrameField;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

public class DeleteDataFrameTransformAction extends Action<DeleteDataFrameTransformAction.Response> {

    public static final DeleteDataFrameTransformAction INSTANCE = new DeleteDataFrameTransformAction();
    public static final String NAME = "cluster:admin/data_frame/delete";

    private DeleteDataFrameTransformAction() {
        super(NAME);
    }

    @Override
    public Response newResponse() {
        return new Response();
    }

    public static class Request extends BaseTasksRequest<Request> implements ToXContentFragment {
        private String id;

        public Request(String id) {
            this.id = ExceptionsHelper.requireNonNull(id, DataFrameField.ID.getPreferredName());
        }

        public Request() {
        }

        public Request(StreamInput in) throws IOException {
            super(in);
            id = in.readString();
        }

        public String getId() {
            return id;
        }

        @Override
        public boolean match(Task task) {
            return task.getDescription().equals(DataFrameField.PERSISTENT_TASK_DESCRIPTION_PREFIX + id);
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeString(id);
        }

        @Override
        public ActionRequestValidationException validate() {
            return null;
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.field(DataFrameField.ID.getPreferredName(), id);
            return builder;
        }

        @Override
        public int hashCode() {
            return Objects.hash(id);
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
            return Objects.equals(id, other.id);
        }
    }

    public static class RequestBuilder
            extends ActionRequestBuilder<DeleteDataFrameTransformAction.Request, DeleteDataFrameTransformAction.Response> {

        protected RequestBuilder(ElasticsearchClient client, DeleteDataFrameTransformAction action) {
            super(client, action, new DeleteDataFrameTransformAction.Request());
        }
    }

    public static class Response extends BaseTasksResponse implements Writeable, ToXContentObject {
        private boolean acknowledged;
        public Response(StreamInput in) throws IOException {
            super(Collections.emptyList(), Collections.emptyList());
            readFrom(in);
        }

        public Response(boolean acknowledged, List<TaskOperationFailure> taskFailures, List<FailedNodeException> nodeFailures) {
            super(taskFailures, nodeFailures);
            this.acknowledged = acknowledged;
        }

        public Response(boolean acknowledged) {
            this(acknowledged, Collections.emptyList(), Collections.emptyList());
        }

        public Response() {
            this(false, Collections.emptyList(), Collections.emptyList());
        }

        public boolean isDeleted() {
            return acknowledged;
        }

        @Override
        public void readFrom(StreamInput in) throws IOException {
            super.readFrom(in);
            acknowledged = in.readBoolean();
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            out.writeBoolean(acknowledged);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            {
                toXContentCommon(builder, params);
                builder.field("acknowledged", acknowledged);
            }
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o)
                return true;
            if (o == null || getClass() != o.getClass())
                return false;
            DeleteDataFrameTransformAction.Response response = (DeleteDataFrameTransformAction.Response) o;
            return super.equals(o) && acknowledged == response.acknowledged;
        }

        @Override
        public int hashCode() {
            return Objects.hash(super.hashCode(), acknowledged);
        }
    }
}
