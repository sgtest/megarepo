/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plugin;

import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.sql.execution.PlanExecutor;
import org.elasticsearch.xpack.sql.session.Configuration;
import org.elasticsearch.xpack.sql.session.Cursors;
import org.elasticsearch.xpack.sql.session.RowSet;
import org.elasticsearch.xpack.sql.session.SchemaRowSet;
import org.elasticsearch.xpack.sql.type.Schema;

import java.util.ArrayList;
import java.util.List;

import static java.util.Collections.unmodifiableList;
import static org.elasticsearch.xpack.sql.plugin.AbstractSqlRequest.Mode.JDBC;

public class TransportSqlQueryAction extends HandledTransportAction<SqlQueryRequest, SqlQueryResponse> {
    private final PlanExecutor planExecutor;
    private final SqlLicenseChecker sqlLicenseChecker;

    @Inject
    public TransportSqlQueryAction(Settings settings, ThreadPool threadPool,
                                   TransportService transportService, ActionFilters actionFilters,
                                   IndexNameExpressionResolver indexNameExpressionResolver,
                                   PlanExecutor planExecutor,
                                   SqlLicenseChecker sqlLicenseChecker) {
        super(settings, SqlQueryAction.NAME, threadPool, transportService, actionFilters, SqlQueryRequest::new,
                indexNameExpressionResolver);

        this.planExecutor = planExecutor;
        this.sqlLicenseChecker = sqlLicenseChecker;
    }

    @Override
    protected void doExecute(SqlQueryRequest request, ActionListener<SqlQueryResponse> listener) {
        sqlLicenseChecker.checkIfSqlAllowed(request.mode());
        operation(planExecutor, request, listener);
    }

    /**
     * Actual implementation of the action. Statically available to support embedded mode.
     */
    public static void operation(PlanExecutor planExecutor, SqlQueryRequest request, ActionListener<SqlQueryResponse> listener) {
        // The configuration is always created however when dealing with the next page, only the timeouts are relevant
        // the rest having default values (since the query is already created)
        Configuration cfg = new Configuration(request.timeZone(), request.fetchSize(), request.requestTimeout(), request.pageTimeout(),
                request.filter());

        if (Strings.hasText(request.cursor()) == false) {
            planExecutor.sql(cfg, request.query(), request.params(),
                    ActionListener.wrap(rowSet -> listener.onResponse(createResponse(request, rowSet)), listener::onFailure));
        } else {
            planExecutor.nextPage(cfg, Cursors.decodeFromString(request.cursor()),
                    ActionListener.wrap(rowSet -> listener.onResponse(createResponse(rowSet, null)), listener::onFailure));
        }
    }

    static SqlQueryResponse createResponse(SqlQueryRequest request, SchemaRowSet rowSet) {
        List<ColumnInfo> columns = new ArrayList<>(rowSet.columnCount());
        for (Schema.Entry entry : rowSet.schema()) {
            if (request.mode() == JDBC) {
                columns.add(new ColumnInfo("", entry.name(), entry.type().esType, entry.type().jdbcType,
                        entry.type().displaySize));
            } else {
                columns.add(new ColumnInfo("", entry.name(), entry.type().esType));
            }
        }
        columns = unmodifiableList(columns);
        return createResponse(rowSet, columns);
    }

    static SqlQueryResponse createResponse(RowSet rowSet, List<ColumnInfo> columns) {
        List<List<Object>> rows = new ArrayList<>();
        rowSet.forEachRow(rowView -> {
            List<Object> row = new ArrayList<>(rowView.columnCount());
            rowView.forEachColumn(row::add);
            rows.add(unmodifiableList(row));
        });

        return new SqlQueryResponse(
                Cursors.encodeToString(Version.CURRENT, rowSet.nextPageCursor()),
                columns,
                rows);
    }
}
