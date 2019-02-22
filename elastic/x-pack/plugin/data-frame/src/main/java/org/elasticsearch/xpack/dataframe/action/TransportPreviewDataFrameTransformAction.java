/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.license.LicenseUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.XPackField;
import org.elasticsearch.xpack.core.dataframe.transform.DataFrameIndexerTransformStats;
import org.elasticsearch.xpack.dataframe.transforms.pivot.Pivot;

import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import java.util.stream.Collectors;

import static org.elasticsearch.xpack.dataframe.transforms.DataFrameIndexer.COMPOSITE_AGGREGATION_NAME;

public class TransportPreviewDataFrameTransformAction extends
    HandledTransportAction<PreviewDataFrameTransformAction.Request, PreviewDataFrameTransformAction.Response> {

    private final XPackLicenseState licenseState;
    private final Client client;
    private final ThreadPool threadPool;

    @Inject
    public TransportPreviewDataFrameTransformAction(TransportService transportService, ActionFilters actionFilters,
                                                    Client client, ThreadPool threadPool, XPackLicenseState licenseState) {
        super(PreviewDataFrameTransformAction.NAME,transportService, actionFilters,
            (Supplier<PreviewDataFrameTransformAction.Request>) PreviewDataFrameTransformAction.Request::new);
        this.licenseState = licenseState;
        this.client = client;
        this.threadPool = threadPool;
    }

    @Override
    protected void doExecute(Task task,
                             PreviewDataFrameTransformAction.Request request,
                             ActionListener<PreviewDataFrameTransformAction.Response> listener) {
        if (!licenseState.isDataFrameAllowed()) {
            listener.onFailure(LicenseUtils.newComplianceException(XPackField.DATA_FRAME));
            return;
        }

        Pivot pivot = new Pivot(request.getConfig().getSource(),
            request.getConfig().getQueryConfig().getQuery(),
            request.getConfig().getPivotConfig());

        getPreview(pivot, ActionListener.wrap(
            previewResponse -> listener.onResponse(new PreviewDataFrameTransformAction.Response(previewResponse)),
            listener::onFailure
        ));
    }

    private void getPreview(Pivot pivot, ActionListener<List<Map<String, Object>>> listener) {
        ClientHelper.executeWithHeadersAsync(threadPool.getThreadContext().getHeaders(),
            ClientHelper.DATA_FRAME_ORIGIN,
            client,
            SearchAction.INSTANCE,
            pivot.buildSearchRequest(null),
            ActionListener.wrap(
                r -> {
                    final CompositeAggregation agg = r.getAggregations().get(COMPOSITE_AGGREGATION_NAME);
                    DataFrameIndexerTransformStats stats = new DataFrameIndexerTransformStats();
                    listener.onResponse(pivot.extractResults(agg, stats).collect(Collectors.toList()));
                },
                listener::onFailure
            ));
    }
}
