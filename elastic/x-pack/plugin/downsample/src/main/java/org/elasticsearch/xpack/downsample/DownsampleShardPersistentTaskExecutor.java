/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.downsample;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.routing.ShardRouting;
import org.elasticsearch.common.util.concurrent.AbstractRunnable;
import org.elasticsearch.index.IndexService;
import org.elasticsearch.index.mapper.TimeSeriesIdFieldMapper;
import org.elasticsearch.index.shard.ShardId;
import org.elasticsearch.indices.IndicesService;
import org.elasticsearch.persistent.AllocatedPersistentTask;
import org.elasticsearch.persistent.PersistentTaskState;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.persistent.PersistentTasksExecutor;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.xpack.core.downsample.DownsampleShardIndexerStatus;
import org.elasticsearch.xpack.core.downsample.DownsampleShardPersistentTaskState;
import org.elasticsearch.xpack.core.downsample.DownsampleShardTask;

import java.util.Arrays;
import java.util.Collection;
import java.util.Map;
import java.util.Objects;

public class DownsampleShardPersistentTaskExecutor extends PersistentTasksExecutor<DownsampleShardTaskParams> {
    private static final Logger logger = LogManager.getLogger(DownsampleShardPersistentTaskExecutor.class);
    private final Client client;
    private final IndicesService indicesService;

    public DownsampleShardPersistentTaskExecutor(
        final Client client,
        final IndicesService indicesService,
        final String taskName,
        final String executorName
    ) {
        super(taskName, executorName);
        this.client = Objects.requireNonNull(client);
        this.indicesService = Objects.requireNonNull(indicesService);
    }

    @Override
    protected void nodeOperation(
        final AllocatedPersistentTask task,
        final DownsampleShardTaskParams params,
        final PersistentTaskState state
    ) {
        // NOTE: query the downsampling target index so that we can start the downsampling task from the latest indexed tsid.
        final SearchRequest searchRequest = new SearchRequest(params.downsampleIndex());
        searchRequest.source().sort(TimeSeriesIdFieldMapper.NAME, SortOrder.DESC).size(1);
        searchRequest.preference("_shards:" + params.shardId().id());
        client.search(
            searchRequest,
            ActionListener.wrap(
                searchResponse -> fork(task, params, searchResponse.getHits().getHits()),
                e -> fork(task, params, new SearchHit[] {})
            )
        );
    }

    private void fork(
        final AllocatedPersistentTask task,
        final DownsampleShardTaskParams params,
        final SearchHit[] lastDownsampledTsidHits
    ) {
        client.threadPool().executor(Downsample.DOWSAMPLE_TASK_THREAD_POOL_NAME).execute(new AbstractRunnable() {
            @Override
            public void onFailure(Exception e) {
                task.markAsFailed(e);
            }

            @Override
            protected void doRun() throws Exception {
                startDownsampleShardIndexer(task, params, lastDownsampledTsidHits);
            }
        });
    }

    private void startDownsampleShardIndexer(
        final AllocatedPersistentTask task,
        final DownsampleShardTaskParams params,
        final SearchHit[] lastDownsampleTsidHits
    ) {
        final DownsampleShardPersistentTaskState initialState = lastDownsampleTsidHits.length == 0
            ? new DownsampleShardPersistentTaskState(DownsampleShardIndexerStatus.INITIALIZED, null)
            : new DownsampleShardPersistentTaskState(
                DownsampleShardIndexerStatus.STARTED,
                Arrays.stream(lastDownsampleTsidHits).findFirst().get().field("_tsid").getValue()
            );
        final DownsampleShardIndexer downsampleShardIndexer = new DownsampleShardIndexer(
            (DownsampleShardTask) task,
            client,
            getIndexService(indicesService, params),
            params.shardId(),
            params.downsampleIndex(),
            params.downsampleConfig(),
            params.metrics(),
            params.labels(),
            initialState
        );
        try {
            downsampleShardIndexer.execute();
            task.markAsCompleted();
        } catch (final DownsampleShardIndexerException e) {
            if (e.isRetriable()) {
                logger.error("Downsampling task [" + task.getPersistentTaskId() + " retriable failure [" + e.getMessage() + "]");
                task.markAsLocallyAborted(e.getMessage());
            } else {
                logger.error("Downsampling task [" + task.getPersistentTaskId() + " non retriable failure [" + e.getMessage() + "]");
                task.markAsFailed(e);
            }
        } catch (final Exception e) {
            logger.error("Downsampling task [" + task.getPersistentTaskId() + " non-retriable failure [" + e.getMessage() + "]");
            task.markAsFailed(e);
        }
    }

    private static IndexService getIndexService(final IndicesService indicesService, final DownsampleShardTaskParams params) {
        return indicesService.indexService(params.shardId().getIndex());
    }

    @Override
    protected AllocatedPersistentTask createTask(
        long id,
        final String type,
        final String action,
        final TaskId parentTaskId,
        final PersistentTasksCustomMetadata.PersistentTask<DownsampleShardTaskParams> taskInProgress,
        final Map<String, String> headers
    ) {
        final DownsampleShardTaskParams params = taskInProgress.getParams();
        return new DownsampleShardTask(
            id,
            type,
            action,
            parentTaskId,
            params.downsampleIndex(),
            params.indexStartTimeMillis(),
            params.indexEndTimeMillis(),
            params.downsampleConfig(),
            headers,
            params.shardId()
        );
    }

    @Override
    public PersistentTasksCustomMetadata.Assignment getAssignment(
        final DownsampleShardTaskParams params,
        final Collection<DiscoveryNode> candidateNodes,
        final ClusterState clusterState
    ) {
        // NOTE: downsampling works by running a task per each shard of the source index.
        // Here we make sure we assign the task to the actual node holding the shard identified by
        // the downsampling task shard id.
        final ShardId shardId = params.shardId();
        final ShardRouting shardRouting = clusterState.routingTable().shardRoutingTable(shardId).primaryShard();
        if (shardRouting.started() == false) {
            return NO_NODE_FOUND;
        }

        return candidateNodes.stream()
            .filter(candidateNode -> candidateNode.getId().equals(shardRouting.currentNodeId()))
            .findAny()
            .map(
                node -> new PersistentTasksCustomMetadata.Assignment(
                    node.getId(),
                    "downsampling using node holding shard [" + shardId + "]"
                )
            )
            .orElse(NO_NODE_FOUND);
    }

    @Override
    public String getExecutor() {
        return Downsample.DOWSAMPLE_TASK_THREAD_POOL_NAME;
    }
}
