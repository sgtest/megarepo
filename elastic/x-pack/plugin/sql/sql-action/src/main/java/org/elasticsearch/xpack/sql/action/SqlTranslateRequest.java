/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.action;

import org.elasticsearch.action.ActionRequestValidationException;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.xpack.sql.proto.Mode;
import org.elasticsearch.xpack.sql.proto.SqlTypedParamValue;
import org.elasticsearch.xpack.sql.proto.SqlQueryRequest;

import java.io.IOException;
import java.util.List;
import java.util.TimeZone;

import static org.elasticsearch.action.ValidateActions.addValidationError;

/**
 * Request for the sql action for translating SQL queries into ES requests
 */
public class SqlTranslateRequest extends AbstractSqlQueryRequest {
    private static final ObjectParser<SqlTranslateRequest, Void> PARSER = objectParser(SqlTranslateRequest::new);

    public SqlTranslateRequest() {
    }

    public SqlTranslateRequest(Mode mode, String query, List<SqlTypedParamValue> params, QueryBuilder filter, TimeZone timeZone,
                               int fetchSize, TimeValue requestTimeout, TimeValue pageTimeout) {
        super(mode, query, params, filter, timeZone, fetchSize, requestTimeout, pageTimeout);
    }

    public SqlTranslateRequest(StreamInput in) throws IOException {
        super(in);
    }

    @Override
    public ActionRequestValidationException validate() {
        ActionRequestValidationException validationException = null;
        if ((false == Strings.hasText(query()))) {
            validationException = addValidationError("query is required", validationException);
        }
        return validationException;
    }

    @Override
    public String getDescription() {
        return "SQL Translate [" + query() + "][" + filter() + "]";
    }

    public static SqlTranslateRequest fromXContent(XContentParser parser, Mode mode) {
        SqlTranslateRequest request = PARSER.apply(parser, null);
        request.mode(mode);
        return request;
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        // This is needed just to test parsing of SqlTranslateRequest, so we can reuse SqlQuerySerialization
        return new SqlQueryRequest(mode(), query(), params(), timeZone(), fetchSize(),
            requestTimeout(), pageTimeout(), filter(), null).toXContent(builder, params);

    }


}
