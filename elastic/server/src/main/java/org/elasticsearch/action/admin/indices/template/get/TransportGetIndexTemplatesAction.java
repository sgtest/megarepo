/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.action.admin.indices.template.get;

import com.carrotsearch.hppc.cursors.ObjectObjectCursor;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeReadAction;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.IndexTemplateMetadata;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.regex.Regex;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

public class TransportGetIndexTemplatesAction extends
    TransportMasterNodeReadAction<GetIndexTemplatesRequest, GetIndexTemplatesResponse> {

    @Inject
    public TransportGetIndexTemplatesAction(TransportService transportService, ClusterService clusterService,
                                            ThreadPool threadPool, ActionFilters actionFilters,
                                            IndexNameExpressionResolver indexNameExpressionResolver) {
        super(GetIndexTemplatesAction.NAME, transportService, clusterService, threadPool, actionFilters,
            GetIndexTemplatesRequest::new, indexNameExpressionResolver, GetIndexTemplatesResponse::new, ThreadPool.Names.SAME);
    }

    @Override
    protected ClusterBlockException checkBlock(GetIndexTemplatesRequest request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_READ);
    }

    @Override
    protected void masterOperation(Task task, GetIndexTemplatesRequest request, ClusterState state,
                                   ActionListener<GetIndexTemplatesResponse> listener) {
        List<IndexTemplateMetadata> results;

        // If we did not ask for a specific name, then we return all templates
        if (request.names().length == 0) {
            results = Arrays.asList(state.metadata().templates().values().toArray(IndexTemplateMetadata.class));
        } else {
            results = new ArrayList<>();
        }

        for (String name : request.names()) {
            if (Regex.isSimpleMatchPattern(name)) {
                for (ObjectObjectCursor<String, IndexTemplateMetadata> entry : state.metadata().templates()) {
                    if (Regex.simpleMatch(name, entry.key)) {
                        results.add(entry.value);
                    }
                }
            } else if (state.metadata().templates().containsKey(name)) {
                results.add(state.metadata().templates().get(name));
            }
        }

        listener.onResponse(new GetIndexTemplatesResponse(results));
    }
}
