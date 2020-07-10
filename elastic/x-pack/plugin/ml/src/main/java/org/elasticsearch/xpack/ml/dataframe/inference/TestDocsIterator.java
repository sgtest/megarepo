/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.ml.dataframe.inference;

import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.sort.FieldSortBuilder;
import org.elasticsearch.search.sort.SortBuilders;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsConfig;
import org.elasticsearch.xpack.ml.dataframe.DestinationIndex;
import org.elasticsearch.xpack.ml.utils.persistence.SearchAfterDocumentsIterator;

import java.util.Objects;

public class TestDocsIterator extends SearchAfterDocumentsIterator<SearchHit> {

    private final DataFrameAnalyticsConfig config;
    private String lastDocId;

    TestDocsIterator(OriginSettingClient client, DataFrameAnalyticsConfig config) {
        super(client, config.getDest().getIndex(), true);
        this.config = Objects.requireNonNull(config);
    }

    @Override
    protected QueryBuilder getQuery() {
        return QueryBuilders.boolQuery().mustNot(
            QueryBuilders.termQuery(config.getDest().getResultsField() + "." + DestinationIndex.IS_TRAINING, true));
    }

    @Override
    protected FieldSortBuilder sortField() {
        return SortBuilders.fieldSort(DestinationIndex.ID_COPY).order(SortOrder.ASC);
    }

    @Override
    protected SearchHit map(SearchHit hit) {
        return hit;
    }

    @Override
    protected Object[] searchAfterFields() {
        return lastDocId == null ? null : new Object[] {lastDocId};
    }

    @Override
    protected void extractSearchAfterFields(SearchHit lastSearchHit) {
        lastDocId = lastSearchHit.getId();
    }

    @Override
    protected SearchResponse executeSearchRequest(SearchRequest searchRequest) {
        return ClientHelper.executeWithHeaders(config.getHeaders(), ClientHelper.ML_ORIGIN, client(),
            () -> client().search(searchRequest).actionGet());
    }
}
