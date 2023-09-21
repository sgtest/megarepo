/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.profiling;

import org.elasticsearch.action.ActionRequest;
import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.action.IndicesRequest;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.common.ParsingException;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.tasks.CancellableTask;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.xcontent.ParseField;
import org.elasticsearch.xcontent.XContentParser;

import java.io.IOException;
import java.util.HashSet;
import java.util.Map;
import java.util.Objects;
import java.util.Set;

import static org.elasticsearch.action.ValidateActions.addValidationError;
import static org.elasticsearch.index.query.AbstractQueryBuilder.parseTopLevelQuery;

/**
 * A request to get profiling details
 */
public class GetStackTracesRequest extends ActionRequest implements IndicesRequest {
    public static final ParseField QUERY_FIELD = new ParseField("query");
    public static final ParseField SAMPLE_SIZE_FIELD = new ParseField("sample_size");

    private QueryBuilder query;

    private Integer sampleSize;

    // We intentionally don't expose this field via the REST API but we can control behavior within Elasticsearch.
    // Once we have migrated all client-side code to dedicated APIs (such as the flamegraph API), we can adjust
    // sample counts by default and remove this flag.
    private Boolean adjustSampleCount;

    public GetStackTracesRequest() {
        this(null, null);
    }

    public GetStackTracesRequest(Integer sampleSize, QueryBuilder query) {
        this.sampleSize = sampleSize;
        this.query = query;
    }

    public GetStackTracesRequest(StreamInput in) throws IOException {
        this.query = in.readOptionalNamedWriteable(QueryBuilder.class);
        this.sampleSize = in.readOptionalInt();
        this.adjustSampleCount = in.readOptionalBoolean();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        out.writeOptionalNamedWriteable(query);
        out.writeOptionalInt(sampleSize);
        out.writeOptionalBoolean(adjustSampleCount);
    }

    public Integer getSampleSize() {
        return sampleSize;
    }

    public QueryBuilder getQuery() {
        return query;
    }

    public boolean isAdjustSampleCount() {
        return Boolean.TRUE.equals(adjustSampleCount);
    }

    public void setAdjustSampleCount(Boolean adjustSampleCount) {
        this.adjustSampleCount = adjustSampleCount;
    }

    public void parseXContent(XContentParser parser) throws IOException {
        XContentParser.Token token = parser.currentToken();
        String currentFieldName = null;
        if (token != XContentParser.Token.START_OBJECT && (token = parser.nextToken()) != XContentParser.Token.START_OBJECT) {
            throw new ParsingException(
                parser.getTokenLocation(),
                "Expected [" + XContentParser.Token.START_OBJECT + "] but found [" + token + "]",
                parser.getTokenLocation()
            );
        }

        while ((token = parser.nextToken()) != XContentParser.Token.END_OBJECT) {
            if (token == XContentParser.Token.FIELD_NAME) {
                currentFieldName = parser.currentName();
            } else if (token.isValue()) {
                if (SAMPLE_SIZE_FIELD.match(currentFieldName, parser.getDeprecationHandler())) {
                    this.sampleSize = parser.intValue();
                } else {
                    throw new ParsingException(
                        parser.getTokenLocation(),
                        "Unknown key for a " + token + " in [" + currentFieldName + "].",
                        parser.getTokenLocation()
                    );
                }
            } else if (token == XContentParser.Token.START_OBJECT) {
                if (QUERY_FIELD.match(currentFieldName, parser.getDeprecationHandler())) {
                    this.query = parseTopLevelQuery(parser);
                }
            } else {
                throw new ParsingException(
                    parser.getTokenLocation(),
                    "Unknown key for a " + token + " in [" + currentFieldName + "].",
                    parser.getTokenLocation()
                );
            }
        }

        token = parser.nextToken();
        if (token != null) {
            throw new ParsingException(parser.getTokenLocation(), "Unexpected token [" + token + "] found after the main object.");
        }
    }

    @Override
    public ActionRequestValidationException validate() {
        ActionRequestValidationException validationException = null;
        if (sampleSize == null) {
            validationException = addValidationError("[" + SAMPLE_SIZE_FIELD.getPreferredName() + "] is mandatory", validationException);
        } else if (sampleSize <= 0) {
            validationException = addValidationError(
                "[" + SAMPLE_SIZE_FIELD.getPreferredName() + "] must be greater or equals than 1, got: " + sampleSize,
                validationException
            );
        }
        return validationException;
    }

    @Override
    public Task createTask(long id, String type, String action, TaskId parentTaskId, Map<String, String> headers) {
        return new CancellableTask(id, type, action, null, parentTaskId, headers) {
            @Override
            public String getDescription() {
                // generating description lazily since the query could be large
                StringBuilder sb = new StringBuilder();
                sb.append("sample_size[").append(sampleSize).append("]");
                if (query == null) {
                    sb.append(", query[]");
                } else {
                    sb.append(", query[").append(Strings.toString(query)).append("]");
                }
                return sb.toString();
            }
        };
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        GetStackTracesRequest that = (GetStackTracesRequest) o;
        return Objects.equals(query, that.query) && Objects.equals(sampleSize, that.sampleSize);
    }

    @Override
    public int hashCode() {
        return Objects.hash(query, sampleSize);
    }

    @Override
    public String[] indices() {
        Set<String> indices = new HashSet<>();
        indices.add("profiling-stacktraces");
        indices.add("profiling-stackframes");
        indices.add("profiling-executables");
        indices.addAll(EventsIndex.indexNames());
        return indices.toArray(new String[0]);
    }

    @Override
    public IndicesOptions indicesOptions() {
        return IndicesOptions.STRICT_EXPAND_OPEN;
    }

    @Override
    public boolean includeDataStreams() {
        return true;
    }
}
