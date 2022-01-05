/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.sql.client;

import org.elasticsearch.xpack.sql.client.JreHttpUrlConnection.ResponseOrException;
import org.elasticsearch.xpack.sql.proto.AbstractSqlRequest;
import org.elasticsearch.xpack.sql.proto.CoreProtocol;
import org.elasticsearch.xpack.sql.proto.MainResponse;
import org.elasticsearch.xpack.sql.proto.Mode;
import org.elasticsearch.xpack.sql.proto.RequestInfo;
import org.elasticsearch.xpack.sql.proto.SqlClearCursorRequest;
import org.elasticsearch.xpack.sql.proto.SqlClearCursorResponse;
import org.elasticsearch.xpack.sql.proto.SqlQueryRequest;
import org.elasticsearch.xpack.sql.proto.SqlQueryResponse;
import org.elasticsearch.xpack.sql.proto.core.Streams;
import org.elasticsearch.xpack.sql.proto.core.TimeValue;
import org.elasticsearch.xpack.sql.proto.core.Tuple;
import org.elasticsearch.xpack.sql.proto.xcontent.ToXContent;
import org.elasticsearch.xpack.sql.proto.xcontent.XContentBuilder;
import org.elasticsearch.xpack.sql.proto.xcontent.XContentParser;
import org.elasticsearch.xpack.sql.proto.xcontent.XContentParserConfiguration;
import org.elasticsearch.xpack.sql.proto.xcontent.XContentType;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.security.PrivilegedAction;
import java.sql.SQLException;
import java.util.function.Function;

import static java.util.Collections.emptyList;
import static java.util.Collections.emptyMap;

/**
 * A specialized high-level REST client with support for SQL-related functions.
 * Similar to JDBC and the underlying HTTP connection, this class is not thread-safe
 * and follows a request-response flow.
 */
public class HttpClient {

    private final ConnectionConfiguration cfg;
    private final XContentType requestBodyContentType;

    public HttpClient(ConnectionConfiguration cfg) {
        this.cfg = cfg;
        this.requestBodyContentType = cfg.binaryCommunication() ? XContentType.CBOR : XContentType.JSON;
    }

    public boolean ping(long timeoutInMs) throws SQLException {
        return head("/", timeoutInMs);
    }

    public MainResponse serverInfo() throws SQLException {
        return get("/", MainResponse::fromXContent);
    }

    public SqlQueryResponse basicQuery(String query, int fetchSize) throws SQLException {
        // TODO allow customizing the time zone - this is what session set/reset/get should be about
        // method called only from CLI
        SqlQueryRequest sqlRequest = new SqlQueryRequest(
            query,
            emptyList(),
            CoreProtocol.TIME_ZONE,
            null,
            fetchSize,
            TimeValue.timeValueMillis(cfg.queryTimeout()),
            TimeValue.timeValueMillis(cfg.pageTimeout()),
            null,
            Boolean.FALSE,
            null,
            new RequestInfo(Mode.CLI, ClientVersion.CURRENT),
            false,
            false,
            cfg.binaryCommunication(),
            emptyMap()
        );
        return query(sqlRequest);
    }

    public SqlQueryResponse query(SqlQueryRequest sqlRequest) throws SQLException {
        return post(CoreProtocol.SQL_QUERY_REST_ENDPOINT, sqlRequest, SqlQueryResponse::fromXContent);
    }

    public SqlQueryResponse nextPage(String cursor) throws SQLException {
        // method called only from CLI
        SqlQueryRequest sqlRequest = new SqlQueryRequest(
            cursor,
            TimeValue.timeValueMillis(cfg.queryTimeout()),
            TimeValue.timeValueMillis(cfg.pageTimeout()),
            new RequestInfo(Mode.CLI),
            cfg.binaryCommunication()
        );
        return post(CoreProtocol.SQL_QUERY_REST_ENDPOINT, sqlRequest, SqlQueryResponse::fromXContent);
    }

    public boolean queryClose(String cursor, Mode mode) throws SQLException {
        SqlClearCursorResponse response = post(
            CoreProtocol.CLEAR_CURSOR_REST_ENDPOINT,
            new SqlClearCursorRequest(cursor, new RequestInfo(mode)),
            SqlClearCursorResponse::fromXContent
        );
        return response.isSucceeded();
    }

    @SuppressWarnings({ "removal" })
    private <Request extends AbstractSqlRequest, Response> Response post(
        String path,
        Request request,
        CheckedFunction<XContentParser, Response, IOException> responseParser
    ) throws SQLException {
        byte[] requestBytes = toXContent(request);
        String query = "error_trace";
        Tuple<XContentType, byte[]> response = java.security.AccessController.doPrivileged(
            (PrivilegedAction<ResponseOrException<Tuple<XContentType, byte[]>>>) () -> JreHttpUrlConnection.http(
                path,
                query,
                cfg,
                con -> con.request(
                    (out) -> out.write(requestBytes),
                    this::readFrom,
                    "POST",
                    requestBodyContentType.mediaTypeWithoutParameters() // "application/cbor" or "application/json"
                )
            )
        ).getResponseOrThrowException();
        return fromXContent(response.v1(), response.v2(), responseParser);
    }

    @SuppressWarnings({ "removal" })
    private boolean head(String path, long timeoutInMs) throws SQLException {
        ConnectionConfiguration pingCfg = new ConnectionConfiguration(
            cfg.baseUri(),
            cfg.connectionString(),
            cfg.validateProperties(),
            cfg.binaryCommunication(),
            cfg.connectTimeout(),
            timeoutInMs,
            cfg.queryTimeout(),
            cfg.pageTimeout(),
            cfg.pageSize(),
            cfg.authUser(),
            cfg.authPass(),
            cfg.sslConfig(),
            cfg.proxyConfig()
        );
        try {
            return java.security.AccessController.doPrivileged(
                (PrivilegedAction<Boolean>) () -> JreHttpUrlConnection.http(path, "error_trace", pingCfg, JreHttpUrlConnection::head)
            );
        } catch (ClientException ex) {
            throw new SQLException("Cannot ping server", ex);
        }
    }

    @SuppressWarnings({ "removal" })
    private <Response> Response get(String path, CheckedFunction<XContentParser, Response, IOException> responseParser)
        throws SQLException {
        Tuple<XContentType, byte[]> response = java.security.AccessController.doPrivileged(
            (PrivilegedAction<ResponseOrException<Tuple<XContentType, byte[]>>>) () -> JreHttpUrlConnection.http(
                path,
                "error_trace",
                cfg,
                con -> con.request(null, this::readFrom, "GET")
            )
        ).getResponseOrThrowException();
        return fromXContent(response.v1(), response.v2(), responseParser);
    }

    private <Request extends ToXContent> byte[] toXContent(Request xContent) {
        try (ByteArrayOutputStream buffer = new ByteArrayOutputStream()) {
            try (XContentBuilder xContentBuilder = new XContentBuilder(requestBodyContentType.xContent(), buffer)) {
                if (xContent.isFragment()) {
                    xContentBuilder.startObject();
                }
                xContent.toXContent(xContentBuilder, ToXContent.EMPTY_PARAMS);
                if (xContent.isFragment()) {
                    xContentBuilder.endObject();
                }
            }
            return buffer.toByteArray();
        } catch (IOException ex) {
            throw new ClientException("Cannot serialize request", ex);
        }
    }

    private Tuple<XContentType, byte[]> readFrom(InputStream inputStream, Function<String, String> headers) {
        String contentType = headers.apply("Content-Type");
        XContentType xContentType = XContentType.fromMediaType(contentType);
        if (xContentType == null) {
            throw new IllegalStateException("Unsupported Content-Type: " + contentType);
        }
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        try {
            Streams.copy(inputStream, out);
        } catch (IOException ex) {
            throw new ClientException("Cannot deserialize response", ex);
        }
        return new Tuple<>(xContentType, out.toByteArray());

    }

    private <Response> Response fromXContent(
        XContentType xContentType,
        byte[] bytesReference,
        CheckedFunction<XContentParser, Response, IOException> responseParser
    ) {
        try (
            InputStream stream = new ByteArrayInputStream(bytesReference);
            XContentParser parser = xContentType.xContent().createParser(XContentParserConfiguration.EMPTY, stream)
        ) {
            return responseParser.apply(parser);
        } catch (IOException ex) {
            throw new ClientException("Cannot parse response", ex);
        }
    }
}
