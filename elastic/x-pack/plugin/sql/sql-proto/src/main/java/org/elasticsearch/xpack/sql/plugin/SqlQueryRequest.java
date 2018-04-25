/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plugin;

import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.query.AbstractQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;

import java.io.IOException;
import java.util.List;
import java.util.Objects;
import java.util.TimeZone;

import static org.elasticsearch.action.ValidateActions.addValidationError;

/**
 * Request to perform an sql query
 */
public class SqlQueryRequest extends AbstractSqlQueryRequest implements ToXContentObject {
    private static final ObjectParser<SqlQueryRequest, Void> PARSER = objectParser(SqlQueryRequest::new);

    public static final ParseField CURSOR = new ParseField("cursor");
    public static final ParseField FILTER = new ParseField("filter");

    static {
        PARSER.declareString(SqlQueryRequest::cursor, CURSOR);
        PARSER.declareObject(SqlQueryRequest::filter,
                (p, c) -> AbstractQueryBuilder.parseInnerQueryBuilder(p), FILTER);
    }

    private String cursor = "";

    public SqlQueryRequest() {
    }

    public SqlQueryRequest(Mode mode, String query, List<SqlTypedParamValue> params, QueryBuilder filter, TimeZone timeZone,
                           int fetchSize, TimeValue requestTimeout, TimeValue pageTimeout, String cursor) {
        super(mode, query, params, filter, timeZone, fetchSize, requestTimeout, pageTimeout);
        this.cursor = cursor;
    }

    @Override
    public ActionRequestValidationException validate() {
        ActionRequestValidationException validationException = null;
        if ((false == Strings.hasText(query())) && Strings.hasText(cursor) == false) {
            validationException = addValidationError("one of [query] or [cursor] is required", validationException);
        }
        return validationException;
    }

    /**
     * The key that must be sent back to SQL to access the next page of
     * results.
     */
    public String cursor() {
        return cursor;
    }

    /**
     * The key that must be sent back to SQL to access the next page of
     * results.
     */
    public SqlQueryRequest cursor(String cursor) {
        if (cursor == null) {
            throw new IllegalArgumentException("cursor may not be null.");
        }
        this.cursor = cursor;
        return this;
    }

    public SqlQueryRequest(StreamInput in) throws IOException {
        super(in);
        cursor = in.readString();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        super.writeTo(out);
        out.writeString(cursor);
    }

    @Override
    public int hashCode() {
        return Objects.hash(super.hashCode(), cursor);
    }

    @Override
    public boolean equals(Object obj) {
        return super.equals(obj) && Objects.equals(cursor, ((SqlQueryRequest) obj).cursor);
    }

    @Override
    public String getDescription() {
        return "SQL [" + query() + "][" + filter() + "]";
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        super.toXContent(builder, params);
        if (cursor != null) {
            builder.field("cursor", cursor);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public boolean isFragment() {
        return false;
    }

    public static SqlQueryRequest fromXContent(XContentParser parser, Mode mode) {
        SqlQueryRequest request =  PARSER.apply(parser, null);
        request.mode(mode);
        return request;
    }

}