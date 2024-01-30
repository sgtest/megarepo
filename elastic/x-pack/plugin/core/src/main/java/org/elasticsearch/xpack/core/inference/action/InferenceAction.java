/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.inference.action;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.TransportVersion;
import org.elasticsearch.TransportVersions;
import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.ActionResponse;
import org.elasticsearch.action.ActionType;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.inference.InferenceResults;
import org.elasticsearch.inference.InferenceServiceResults;
import org.elasticsearch.inference.InputType;
import org.elasticsearch.inference.TaskType;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.xcontent.ObjectParser;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xcontent.ToXContentObject;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xpack.core.inference.results.LegacyTextEmbeddingResults;
import org.elasticsearch.xpack.core.inference.results.SparseEmbeddingResults;
import org.elasticsearch.xpack.core.ml.inference.results.TextExpansionResults;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;

public class InferenceAction extends ActionType<InferenceAction.Response> {

    public static final InferenceAction INSTANCE = new InferenceAction();
    public static final String NAME = "cluster:monitor/xpack/inference";

    public InferenceAction() {
        super(NAME);
    }

    public static class Request extends ActionRequest {

        public static final ParseField INPUT = new ParseField("input");
        public static final ParseField TASK_SETTINGS = new ParseField("task_settings");

        static final ObjectParser<Request.Builder, Void> PARSER = new ObjectParser<>(NAME, Request.Builder::new);
        static {
            // TODO timeout
            PARSER.declareStringArray(Request.Builder::setInput, INPUT);
            PARSER.declareObject(Request.Builder::setTaskSettings, (p, c) -> p.mapOrdered(), TASK_SETTINGS);
        }

        public static Request parseRequest(String inferenceEntityId, String taskType, XContentParser parser) {
            Request.Builder builder = PARSER.apply(parser, null);
            builder.setInferenceEntityId(inferenceEntityId);
            builder.setTaskType(taskType);
            // For rest requests we won't know what the input type is
            builder.setInputType(InputType.UNSPECIFIED);
            return builder.build();
        }

        private final TaskType taskType;
        private final String inferenceEntityId;
        private final List<String> input;
        private final Map<String, Object> taskSettings;
        private final InputType inputType;

        public Request(
            TaskType taskType,
            String inferenceEntityId,
            List<String> input,
            Map<String, Object> taskSettings,
            InputType inputType
        ) {
            this.taskType = taskType;
            this.inferenceEntityId = inferenceEntityId;
            this.input = input;
            this.taskSettings = taskSettings;
            this.inputType = inputType;
        }

        public Request(StreamInput in) throws IOException {
            super(in);
            this.taskType = TaskType.fromStream(in);
            this.inferenceEntityId = in.readString();
            if (in.getTransportVersion().onOrAfter(TransportVersions.INFERENCE_MULTIPLE_INPUTS)) {
                this.input = in.readStringCollectionAsList();
            } else {
                this.input = List.of(in.readString());
            }
            this.taskSettings = in.readGenericMap();
            if (in.getTransportVersion().onOrAfter(TransportVersions.ML_INFERENCE_REQUEST_INPUT_TYPE_ADDED)) {
                this.inputType = in.readEnum(InputType.class);
            } else {
                this.inputType = InputType.UNSPECIFIED;
            }
        }

        public TaskType getTaskType() {
            return taskType;
        }

        public String getInferenceEntityId() {
            return inferenceEntityId;
        }

        public List<String> getInput() {
            return input;
        }

        public Map<String, Object> getTaskSettings() {
            return taskSettings;
        }

        public InputType getInputType() {
            return inputType;
        }

        @Override
        public ActionRequestValidationException validate() {
            if (input == null) {
                var e = new ActionRequestValidationException();
                e.addValidationError("missing input");
                return e;
            }
            if (input.isEmpty()) {
                var e = new ActionRequestValidationException();
                e.addValidationError("input array is empty");
                return e;
            }
            return null;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            super.writeTo(out);
            taskType.writeTo(out);
            out.writeString(inferenceEntityId);
            if (out.getTransportVersion().onOrAfter(TransportVersions.INFERENCE_MULTIPLE_INPUTS)) {
                out.writeStringCollection(input);
            } else {
                out.writeString(input.get(0));
            }
            out.writeGenericMap(taskSettings);
            // in version ML_INFERENCE_REQUEST_INPUT_TYPE_ADDED the input type enum was added, so we only want to write the enum if we're
            // at that version or later
            if (out.getTransportVersion().onOrAfter(TransportVersions.ML_INFERENCE_REQUEST_INPUT_TYPE_ADDED)) {
                out.writeEnum(getInputTypeToWrite(out.getTransportVersion()));
            }
        }

        private InputType getInputTypeToWrite(TransportVersion version) {
            // in version ML_INFERENCE_REQUEST_INPUT_TYPE_UNSPECIFIED_ADDED the UNSPECIFIED value was added, so if we're before that
            // version other nodes won't know about it, so set it to INGEST instead
            if (version.before(TransportVersions.ML_INFERENCE_REQUEST_INPUT_TYPE_UNSPECIFIED_ADDED) && inputType == InputType.UNSPECIFIED) {
                return InputType.INGEST;
            }
            return inputType;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Request request = (Request) o;
            return taskType == request.taskType
                && Objects.equals(inferenceEntityId, request.inferenceEntityId)
                && Objects.equals(input, request.input)
                && Objects.equals(taskSettings, request.taskSettings)
                && Objects.equals(inputType, request.inputType);
        }

        @Override
        public int hashCode() {
            return Objects.hash(taskType, inferenceEntityId, input, taskSettings, inputType);
        }

        public static class Builder {

            private TaskType taskType;
            private String inferenceEntityId;
            private List<String> input;
            private InputType inputType = InputType.UNSPECIFIED;
            private Map<String, Object> taskSettings = Map.of();

            private Builder() {}

            public Builder setInferenceEntityId(String inferenceEntityId) {
                this.inferenceEntityId = Objects.requireNonNull(inferenceEntityId);
                return this;
            }

            public Builder setTaskType(String taskTypeStr) {
                try {
                    TaskType taskType = TaskType.fromString(taskTypeStr);
                    this.taskType = Objects.requireNonNull(taskType);
                } catch (IllegalArgumentException e) {
                    throw new ElasticsearchStatusException("Unknown task_type [{}]", RestStatus.BAD_REQUEST, taskTypeStr);
                }
                return this;
            }

            public Builder setInput(List<String> input) {
                this.input = input;
                return this;
            }

            public Builder setInputType(InputType inputType) {
                this.inputType = inputType;
                return this;
            }

            public Builder setTaskSettings(Map<String, Object> taskSettings) {
                this.taskSettings = taskSettings;
                return this;
            }

            public Request build() {
                return new Request(taskType, inferenceEntityId, input, taskSettings, inputType);
            }
        }
    }

    public static class Response extends ActionResponse implements ToXContentObject {

        private final InferenceServiceResults results;

        public Response(InferenceServiceResults results) {
            this.results = results;
        }

        public Response(StreamInput in) throws IOException {
            super(in);
            if (in.getTransportVersion().onOrAfter(TransportVersions.INFERENCE_SERVICE_RESULTS_ADDED)) {
                results = in.readNamedWriteable(InferenceServiceResults.class);
            } else if (in.getTransportVersion().onOrAfter(TransportVersions.INFERENCE_MULTIPLE_INPUTS)) {
                // This could be List<InferenceResults> aka List<TextEmbeddingResults> from ml plugin for
                // hugging face elser and elser or the legacy format for openai
                results = transformToServiceResults(in.readNamedWriteableCollectionAsList(InferenceResults.class));
            } else {
                // It should only be InferenceResults aka TextEmbeddingResults from ml plugin for
                // hugging face elser and elser
                results = transformToServiceResults(List.of(in.readNamedWriteable(InferenceResults.class)));
            }
        }

        @SuppressWarnings("deprecation")
        public static InferenceServiceResults transformToServiceResults(List<? extends InferenceResults> parsedResults) {
            if (parsedResults.isEmpty()) {
                throw new ElasticsearchStatusException(
                    "Failed to transform results to response format, expected a non-empty list, please remove and re-add the service",
                    RestStatus.INTERNAL_SERVER_ERROR
                );
            }

            if (parsedResults.get(0) instanceof LegacyTextEmbeddingResults openaiResults) {
                if (parsedResults.size() > 1) {
                    throw new ElasticsearchStatusException(
                        "Failed to transform results to response format, malformed text embedding result,"
                            + " please remove and re-add the service",
                        RestStatus.INTERNAL_SERVER_ERROR
                    );
                }

                return openaiResults.transformToTextEmbeddingResults();
            } else if (parsedResults.get(0) instanceof TextExpansionResults) {
                return transformToSparseEmbeddingResult(parsedResults);
            } else {
                throw new ElasticsearchStatusException(
                    "Failed to transform results to response format, unknown embedding type received,"
                        + " please remove and re-add the service",
                    RestStatus.INTERNAL_SERVER_ERROR
                );
            }
        }

        private static SparseEmbeddingResults transformToSparseEmbeddingResult(List<? extends InferenceResults> parsedResults) {
            List<TextExpansionResults> textExpansionResults = new ArrayList<>(parsedResults.size());

            for (InferenceResults result : parsedResults) {
                if (result instanceof TextExpansionResults textExpansion) {
                    textExpansionResults.add(textExpansion);
                } else {
                    throw new ElasticsearchStatusException(
                        "Failed to transform results to response format, please remove and re-add the service",
                        RestStatus.INTERNAL_SERVER_ERROR
                    );
                }
            }

            return SparseEmbeddingResults.of(textExpansionResults);
        }

        public InferenceServiceResults getResults() {
            return results;
        }

        @Override
        public void writeTo(StreamOutput out) throws IOException {
            if (out.getTransportVersion().onOrAfter(TransportVersions.INFERENCE_SERVICE_RESULTS_ADDED)) {
                out.writeNamedWriteable(results);
            } else if (out.getTransportVersion().onOrAfter(TransportVersions.INFERENCE_MULTIPLE_INPUTS)) {
                // This includes the legacy openai response format of List<TextEmbedding> and hugging face elser and elser
                out.writeNamedWriteableCollection(results.transformToLegacyFormat());
            } else {
                out.writeNamedWriteable(results.transformToLegacyFormat().get(0));
            }
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            results.toXContent(builder, params);
            builder.endObject();
            return builder;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (o == null || getClass() != o.getClass()) return false;
            Response response = (Response) o;
            return Objects.equals(results, response.results);
        }

        @Override
        public int hashCode() {
            return Objects.hash(results);
        }
    }
}
