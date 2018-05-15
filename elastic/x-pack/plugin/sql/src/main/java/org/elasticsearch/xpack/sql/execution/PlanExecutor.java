/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.execution;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.xpack.sql.analysis.analyzer.PreAnalyzer;
import org.elasticsearch.xpack.sql.analysis.index.IndexResolver;
import org.elasticsearch.xpack.sql.execution.search.SourceGenerator;
import org.elasticsearch.xpack.sql.expression.function.FunctionRegistry;
import org.elasticsearch.xpack.sql.optimizer.Optimizer;
import org.elasticsearch.xpack.sql.plan.physical.EsQueryExec;
import org.elasticsearch.xpack.sql.planner.Planner;
import org.elasticsearch.xpack.sql.planner.PlanningException;
import org.elasticsearch.xpack.sql.proto.SqlTypedParamValue;
import org.elasticsearch.xpack.sql.session.Configuration;
import org.elasticsearch.xpack.sql.session.Cursor;
import org.elasticsearch.xpack.sql.session.RowSet;
import org.elasticsearch.xpack.sql.session.SchemaRowSet;
import org.elasticsearch.xpack.sql.session.SqlSession;

import java.util.List;

public class PlanExecutor {
    private final Client client;
    private final NamedWriteableRegistry writableRegistry;

    private final FunctionRegistry functionRegistry;

    private final IndexResolver indexResolver;
    private final PreAnalyzer preAnalyzer;
    private final Optimizer optimizer;
    private final Planner planner;

    public PlanExecutor(Client client, IndexResolver indexResolver, NamedWriteableRegistry writeableRegistry) {
        this.client = client;
        this.writableRegistry = writeableRegistry;

        this.indexResolver = indexResolver;
        this.functionRegistry = new FunctionRegistry();

        this.preAnalyzer = new PreAnalyzer();
        this.optimizer = new Optimizer();
        this.planner = new Planner();
    }

    public NamedWriteableRegistry writableRegistry() {
        return writableRegistry;
    }

    private SqlSession newSession(Configuration cfg) {
        return new SqlSession(cfg, client, functionRegistry, indexResolver, preAnalyzer, optimizer, planner);
    }

    public void searchSource(Configuration cfg, String sql, List<SqlTypedParamValue> params, ActionListener<SearchSourceBuilder> listener) {
        newSession(cfg).sqlExecutable(sql, params, ActionListener.wrap(exec -> {
            if (exec instanceof EsQueryExec) {
                EsQueryExec e = (EsQueryExec) exec;
                listener.onResponse(SourceGenerator.sourceBuilder(e.queryContainer(), cfg.filter(), cfg.pageSize()));
            } else {
                listener.onFailure(new PlanningException("Cannot generate a query DSL for {}", sql));
            }
        }, listener::onFailure));
    }

    public void sql(Configuration cfg, String sql, List<SqlTypedParamValue> params, ActionListener<SchemaRowSet> listener) {
        newSession(cfg).sql(sql, params, listener);
    }

    public void nextPage(Configuration cfg, Cursor cursor, ActionListener<RowSet> listener) {
        cursor.nextPage(cfg, client, writableRegistry, listener);
    }

    public void cleanCursor(Configuration cfg, Cursor cursor, ActionListener<Boolean> listener) {
        cursor.clear(cfg, client, listener);
    }
}
