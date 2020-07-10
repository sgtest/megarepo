/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.eql.plan.physical;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.search.sort.SortBuilder;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.xpack.eql.execution.search.BasicQueryClient;
import org.elasticsearch.xpack.eql.execution.search.QueryRequest;
import org.elasticsearch.xpack.eql.execution.search.ReverseListener;
import org.elasticsearch.xpack.eql.execution.search.SourceGenerator;
import org.elasticsearch.xpack.eql.querydsl.container.QueryContainer;
import org.elasticsearch.xpack.eql.session.EqlConfiguration;
import org.elasticsearch.xpack.eql.session.EqlSession;
import org.elasticsearch.xpack.eql.session.Payload;
import org.elasticsearch.xpack.ql.expression.Attribute;
import org.elasticsearch.xpack.ql.tree.NodeInfo;
import org.elasticsearch.xpack.ql.tree.Source;

import java.util.List;
import java.util.Objects;

public class EsQueryExec extends LeafExec {

    private final List<Attribute> output;
    private final QueryContainer queryContainer;

    public EsQueryExec(Source source, List<Attribute> output, QueryContainer queryContainer) {
        super(source);
        this.output = output;
        this.queryContainer = queryContainer;
    }

    @Override
    protected NodeInfo<EsQueryExec> info() {
        return NodeInfo.create(this, EsQueryExec::new, output, queryContainer);
    }

    public EsQueryExec with(QueryContainer queryContainer) {
        return new EsQueryExec(source(), output, queryContainer);
    }

    @Override
    public List<Attribute> output() {
        return output;
    }

    public QueryRequest queryRequest(EqlSession session) {
        EqlConfiguration cfg = session.configuration();
        // by default use the configuration size
        // join/sequence queries will want to override this
        SearchSourceBuilder sourceBuilder = SourceGenerator.sourceBuilder(queryContainer, cfg.filter());
        return () -> sourceBuilder;
    }

    @Override
    public void execute(EqlSession session, ActionListener<Payload> listener) {
        QueryRequest request = queryRequest(session);
        listener = shouldReverse(request) ? new ReverseListener(listener) : listener;
        new BasicQueryClient(session).query(request, listener);
    }

    private boolean shouldReverse(QueryRequest query) {
        SearchSourceBuilder searchSource = query.searchSource();
        // since all results need to be ASC, use this hack to figure out whether the results need to be flipped
        for (SortBuilder<?> sort : searchSource.sorts()) {
            if (sort.order() == SortOrder.DESC) {
                return true;
            }
        }
        return false;
    }

    @Override
    public int hashCode() {
        return Objects.hash(queryContainer, output);
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) {
            return true;
        }

        if (obj == null || getClass() != obj.getClass()) {
            return false;
        }

        EsQueryExec other = (EsQueryExec) obj;
        return Objects.equals(queryContainer, other.queryContainer)
                && Objects.equals(output, other.output);
    }

    @Override
    public String nodeString() {
        return nodeName() + "[" + queryContainer + "]";
    }

    public QueryContainer queryContainer() {
        return queryContainer;
    }
}