/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.plugin;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.sql.execution.PlanExecutor;
import org.elasticsearch.xpack.sql.session.Configuration;
import org.elasticsearch.xpack.sql.session.Cursor;
import org.elasticsearch.xpack.sql.session.Cursors;

import static org.elasticsearch.xpack.sql.plugin.SqlClearCursorAction.NAME;

public class TransportSqlClearCursorAction extends HandledTransportAction<SqlClearCursorRequest, SqlClearCursorResponse> {
    private final PlanExecutor planExecutor;
    private final SqlLicenseChecker sqlLicenseChecker;

    @Inject
    public TransportSqlClearCursorAction(Settings settings, ThreadPool threadPool,
                           TransportService transportService, ActionFilters actionFilters,
                           IndexNameExpressionResolver indexNameExpressionResolver,
                           PlanExecutor planExecutor,
                           SqlLicenseChecker sqlLicenseChecker) {
        super(settings, NAME, threadPool, transportService, actionFilters, SqlClearCursorRequest::new,
                indexNameExpressionResolver);
        this.planExecutor = planExecutor;
        this.sqlLicenseChecker = sqlLicenseChecker;
    }

    @Override
    protected void doExecute(SqlClearCursorRequest request, ActionListener<SqlClearCursorResponse> listener) {
        sqlLicenseChecker.checkIfSqlAllowed(request.mode());
        operation(planExecutor, request, listener);
    }

    public static void operation(PlanExecutor planExecutor, SqlClearCursorRequest request,
            ActionListener<SqlClearCursorResponse> listener) {
        Cursor cursor = Cursors.decodeFromString(request.getCursor());
        planExecutor.cleanCursor(Configuration.DEFAULT, cursor, ActionListener.wrap(
                success -> listener.onResponse(new SqlClearCursorResponse(success)), listener::onFailure));
    }
}

