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

/**
 * Transport action for translating SQL queries into ES requests
 */
public class TransportSqlTranslateAction extends HandledTransportAction<SqlTranslateRequest, SqlTranslateResponse> {
    private final PlanExecutor planExecutor;
    private final SqlLicenseChecker sqlLicenseChecker;

    @Inject
    public TransportSqlTranslateAction(Settings settings, ThreadPool threadPool,
                                       TransportService transportService, ActionFilters actionFilters,
                                       IndexNameExpressionResolver indexNameExpressionResolver,
                                       PlanExecutor planExecutor,
                                       SqlLicenseChecker sqlLicenseChecker) {
        super(settings, SqlTranslateAction.NAME, threadPool, transportService, actionFilters,
                SqlTranslateRequest::new, indexNameExpressionResolver);

        this.planExecutor = planExecutor;
        this.sqlLicenseChecker = sqlLicenseChecker;
    }

    @Override
    protected void doExecute(SqlTranslateRequest request, ActionListener<SqlTranslateResponse> listener) {
        sqlLicenseChecker.checkIfSqlAllowed(request.mode());

        Configuration cfg = new Configuration(request.timeZone(), request.fetchSize(),
                request.requestTimeout(), request.pageTimeout(), request.filter());

        planExecutor.searchSource(cfg, request.query(), request.params(), ActionListener.wrap(
                searchSourceBuilder -> listener.onResponse(new SqlTranslateResponse(searchSourceBuilder)), listener::onFailure));
    }
}
