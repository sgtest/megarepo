/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.bulk.BulkAction;
import org.elasticsearch.action.bulk.BulkRequestBuilder;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.HandledTransportAction;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.ToXContent;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ml.MlMetaIndex;
import org.elasticsearch.xpack.core.ml.action.PutFilterAction;
import org.elasticsearch.xpack.core.ml.job.config.MlFilter;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.job.JobManager;

import java.io.IOException;
import java.util.Collections;
import java.util.function.Supplier;

import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

public class TransportPutFilterAction extends HandledTransportAction<PutFilterAction.Request, PutFilterAction.Response> {

    private final Client client;
    private final JobManager jobManager;

    @Inject
    public TransportPutFilterAction(Settings settings, TransportService transportService, ActionFilters actionFilters,
                                    Client client, JobManager jobManager) {
        super(settings, PutFilterAction.NAME, transportService, actionFilters,
            (Supplier<PutFilterAction.Request>) PutFilterAction.Request::new);
        this.client = client;
        this.jobManager = jobManager;
    }

    @Override
    protected void doExecute(PutFilterAction.Request request, ActionListener<PutFilterAction.Response> listener) {
        MlFilter filter = request.getFilter();
        IndexRequest indexRequest = new IndexRequest(MlMetaIndex.INDEX_NAME, MlMetaIndex.TYPE, filter.documentId());
        try (XContentBuilder builder = XContentFactory.jsonBuilder()) {
            ToXContent.MapParams params = new ToXContent.MapParams(Collections.singletonMap(MlMetaIndex.INCLUDE_TYPE_KEY, "true"));
            indexRequest.source(filter.toXContent(builder, params));
        } catch (IOException e) {
            throw new IllegalStateException("Failed to serialise filter with id [" + filter.getId() + "]", e);
        }
        BulkRequestBuilder bulkRequestBuilder = client.prepareBulk();
        bulkRequestBuilder.add(indexRequest);
        bulkRequestBuilder.setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);

        executeAsyncWithOrigin(client, ML_ORIGIN, BulkAction.INSTANCE, bulkRequestBuilder.request(),
                new ActionListener<BulkResponse>() {
                    @Override
                    public void onResponse(BulkResponse indexResponse) {
                        jobManager.updateProcessOnFilterChanged(filter);
                        listener.onResponse(new PutFilterAction.Response(filter));
                    }

                    @Override
                    public void onFailure(Exception e) {
                        listener.onFailure(ExceptionsHelper.serverError("Error putting filter with id [" + filter.getId() + "]", e));
                    }
                });
    }
}
