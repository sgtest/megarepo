/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.transform;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.protocol.xpack.XPackUsageRequest;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.ClientHelper;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureAction;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureResponse;
import org.elasticsearch.xpack.core.action.XPackUsageFeatureTransportAction;
import org.elasticsearch.xpack.core.transform.TransformFeatureSetUsage;
import org.elasticsearch.xpack.core.transform.TransformField;
import org.elasticsearch.xpack.core.transform.transforms.TransformConfig;
import org.elasticsearch.xpack.core.transform.transforms.TransformIndexerStats;
import org.elasticsearch.xpack.core.transform.transforms.TransformState;
import org.elasticsearch.xpack.core.transform.transforms.TransformTaskParams;
import org.elasticsearch.xpack.core.transform.transforms.TransformTaskState;
import org.elasticsearch.xpack.core.transform.transforms.persistence.TransformInternalIndexConstants;

import java.util.Arrays;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

public class TransformUsageTransportAction extends XPackUsageFeatureTransportAction {

    private static final Logger logger = LogManager.getLogger(TransformUsageTransportAction.class);

    private final Client client;

    @Inject
    public TransformUsageTransportAction(
        TransportService transportService,
        ClusterService clusterService,
        ThreadPool threadPool,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver,
        Client client
    ) {
        super(
            XPackUsageFeatureAction.TRANSFORM.name(),
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            indexNameExpressionResolver
        );
        this.client = client;
    }

    @Override
    protected void masterOperation(
        Task task,
        XPackUsageRequest request,
        ClusterState state,
        ActionListener<XPackUsageFeatureResponse> listener
    ) {
        PersistentTasksCustomMetadata taskMetadata = PersistentTasksCustomMetadata.getPersistentTasksCustomMetadata(state);
        Collection<PersistentTasksCustomMetadata.PersistentTask<?>> transformTasks = taskMetadata == null
            ? Collections.emptyList()
            : taskMetadata.findTasks(TransformTaskParams.NAME, (t) -> true);
        final int taskCount = transformTasks.size();
        final Map<String, Long> transformsCountByState = new HashMap<>();
        for (PersistentTasksCustomMetadata.PersistentTask<?> transformTask : transformTasks) {
            TransformState transformState = (TransformState) transformTask.getState();
            TransformTaskState taskState = transformState.getTaskState();
            if (taskState != null) {
                transformsCountByState.merge(taskState.value(), 1L, Long::sum);
            }
        }

        ActionListener<TransformIndexerStats> totalStatsListener = ActionListener.wrap(statSummations -> {
            var usage = new TransformFeatureSetUsage(transformsCountByState, statSummations);
            listener.onResponse(new XPackUsageFeatureResponse(usage));
        }, listener::onFailure);

        ActionListener<SearchResponse> totalTransformCountListener = ActionListener.wrap(transformCountSuccess -> {
            if (transformCountSuccess.getShardFailures().length > 0) {
                logger.error(
                    "total transform count search returned shard failures: {}",
                    Arrays.toString(transformCountSuccess.getShardFailures())
                );
            }
            long totalTransforms = transformCountSuccess.getHits().getTotalHits().value;
            if (totalTransforms == 0) {
                var usage = new TransformFeatureSetUsage(transformsCountByState, new TransformIndexerStats());
                listener.onResponse(new XPackUsageFeatureResponse(usage));
                return;
            }
            transformsCountByState.merge(TransformTaskState.STOPPED.value(), totalTransforms - taskCount, Long::sum);
            TransformInfoTransportAction.getStatisticSummations(client, totalStatsListener);
        }, transformCountFailure -> {
            if (transformCountFailure instanceof ResourceNotFoundException) {
                TransformInfoTransportAction.getStatisticSummations(client, totalStatsListener);
            } else {
                listener.onFailure(transformCountFailure);
            }
        });

        SearchRequest totalTransformCount = client.prepareSearch(
            TransformInternalIndexConstants.INDEX_NAME_PATTERN,
            TransformInternalIndexConstants.INDEX_NAME_PATTERN_DEPRECATED
        )
            .setTrackTotalHits(true)
            .setQuery(
                QueryBuilders.constantScoreQuery(
                    QueryBuilders.boolQuery()
                        .filter(QueryBuilders.termQuery(TransformField.INDEX_DOC_TYPE.getPreferredName(), TransformConfig.NAME))
                )
            )
            .request();

        ClientHelper.executeAsyncWithOrigin(
            client.threadPool().getThreadContext(),
            ClientHelper.TRANSFORM_ORIGIN,
            totalTransformCount,
            totalTransformCountListener,
            client::search
        );
    }
}
