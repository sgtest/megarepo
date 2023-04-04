/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.application.search.action;

import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.action.DocWriteResponse;
import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.StatusToXContentObject;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.application.search.SearchApplication;

import java.io.IOException;
import java.util.Objects;

import static org.elasticsearch.action.ValidateActions.addValidationError;

public class PutSearchApplicationAction extends ActionType<PutSearchApplicationAction.Response> {

    public static final PutSearchApplicationAction INSTANCE = new PutSearchApplicationAction();
    public static final String NAME = "cluster:admin/xpack/application/search_application/put";

    public PutSearchApplicationAction() {
        super(NAME, PutSearchApplicationAction.Response::new);
    }

    public static class Request extends ActionRequest {

        private final SearchApplication searchApp;
        private final boolean create;

        public Request(StreamInput in) throws IOException {
            super(in);
            this.searchApp = new SearchApplication(in);
            this.create = in.readBoolean();
        }

        public Request(String name, boolean create, BytesReference content, XContentType contentType) {
            this.searchApp = SearchApplication.fromXContentBytes(name, content, contentType);
            this.create = create;
        }

        public Request(SearchApplication app, boolean create) {
            this.searchApp = app;
            this.create = create;
        }

        @Override
        public ActionRequestValidationException validate() {
            ActionRequestValidationException validationException = null;

            if (searchApp.indices().length == 0) {
                validationException = addValidationError("indices are missing", validationException);
            }

            return validationException;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            searchApp.writeTo(out);
            out.writeBoolean(create);
        }

        public SearchApplication getSearchApplication() {
            return searchApp;
        }

        public boolean create() {
            return create;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Request that = (Request) o;
            return Objects.equals(searchApp, that.searchApp) && create == that.create;
        }

        @Override
        public int hashCode() {
            return Objects.hash(searchApp, create);
        }
    }

    public static class Response extends ActionResponse implements StatusToXContentObject {

        final DocWriteResponse.Result result;

        public Response(StreamInput in) throws IOException {
            super(in);
            result = DocWriteResponse.Result.readFrom(in);
        }

        public Response(DocWriteResponse.Result result) {
            this.result = result;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            this.result.writeTo(out);
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field("result", this.result.getLowercase());
            builder.endObject();
            return builder;
        }

        @Override
        public RestStatus status() {
            return switch (result) {
                case CREATED -> RestStatus.CREATED;
                case NOT_FOUND -> RestStatus.NOT_FOUND;
                default -> RestStatus.OK;
            };
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Response that = (Response) o;
            return Objects.equals(result, that.result);
        }

        @Override
        public int hashCode() {
            return Objects.hash(result);
        }

    }

}
