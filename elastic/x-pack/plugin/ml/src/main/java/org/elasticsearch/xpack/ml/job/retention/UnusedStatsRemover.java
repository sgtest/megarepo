/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.job.retention;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.common.Strings;
import org.elasticsearch.index.query.BoolQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.index.reindex.DeleteByQueryAction;
import org.elasticsearch.index.reindex.DeleteByQueryRequest;
import org.elasticsearch.tasks.TaskId;
import org.elasticsearch.xpack.core.ml.MlConfigIndex;
import org.elasticsearch.xpack.core.ml.MlStatsIndex;
import org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsConfig;
import org.elasticsearch.xpack.core.ml.dataframe.stats.Fields;
import org.elasticsearch.xpack.core.ml.inference.TrainedModelConfig;
import org.elasticsearch.xpack.core.ml.inference.persistence.InferenceIndexConstants;
import org.elasticsearch.xpack.ml.inference.persistence.TrainedModelProvider;
import org.elasticsearch.xpack.ml.utils.persistence.DocIdBatchedDocumentIterator;

import java.util.Deque;
import java.util.HashSet;
import java.util.Objects;
import java.util.Set;
import java.util.function.Supplier;

/**
 * If for any reason a job or trained model is deleted but some of its stats documents
 * are left behind, this class deletes any unused documents stored
 * in the .ml-stats* indices.
 */
public class UnusedStatsRemover implements MlDataRemover {

    private static final Logger LOGGER = LogManager.getLogger(UnusedStatsRemover.class);

    private final OriginSettingClient client;
    private final TaskId parentTaskId;

    public UnusedStatsRemover(OriginSettingClient client, TaskId parentTaskId) {
        this.client = Objects.requireNonNull(client);
        this.parentTaskId = Objects.requireNonNull(parentTaskId);
    }

    @Override
    public void remove(float requestsPerSec, ActionListener<Boolean> listener, Supplier<Boolean> isTimedOutSupplier) {
        try {
            if (isTimedOutSupplier.get()) {
                listener.onResponse(false);
                return;
            }
            BoolQueryBuilder queryBuilder = QueryBuilders.boolQuery()
                .mustNot(QueryBuilders.termsQuery(Fields.JOB_ID.getPreferredName(), getDataFrameAnalyticsJobIds()))
                .mustNot(QueryBuilders.termsQuery(TrainedModelConfig.MODEL_ID.getPreferredName(), getTrainedModelIds()));

            if (isTimedOutSupplier.get()) {
                listener.onResponse(false);
                return;
            }
            executeDeleteUnusedStatsDocs(queryBuilder, requestsPerSec, listener);
        } catch (Exception e) {
            listener.onFailure(e);
        }
    }

    private Set<String> getDataFrameAnalyticsJobIds() {
        Set<String> jobIds = new HashSet<>();

        DocIdBatchedDocumentIterator iterator = new DocIdBatchedDocumentIterator(client, MlConfigIndex.indexName(),
            QueryBuilders.termQuery(DataFrameAnalyticsConfig.CONFIG_TYPE.getPreferredName(), DataFrameAnalyticsConfig.TYPE));
        while (iterator.hasNext()) {
            Deque<String> docIds = iterator.next();
            docIds.stream().map(DataFrameAnalyticsConfig::extractJobIdFromDocId).filter(Objects::nonNull).forEach(jobIds::add);
        }
        return jobIds;
    }

    private Set<String> getTrainedModelIds() {
        Set<String> modelIds = new HashSet<>(TrainedModelProvider.MODELS_STORED_AS_RESOURCE);

        DocIdBatchedDocumentIterator iterator = new DocIdBatchedDocumentIterator(client, InferenceIndexConstants.INDEX_PATTERN,
            QueryBuilders.termQuery(InferenceIndexConstants.DOC_TYPE.getPreferredName(), TrainedModelConfig.NAME));
        while (iterator.hasNext()) {
            Deque<String> docIds = iterator.next();
            docIds.stream().filter(Objects::nonNull).forEach(modelIds::add);
        }
        return modelIds;
    }

    private void executeDeleteUnusedStatsDocs(QueryBuilder dbq, float requestsPerSec, ActionListener<Boolean> listener) {
        DeleteByQueryRequest deleteByQueryRequest = new DeleteByQueryRequest(MlStatsIndex.indexPattern())
            .setIndicesOptions(IndicesOptions.lenientExpandOpen())
            .setAbortOnVersionConflict(false)
            .setRequestsPerSecond(requestsPerSec)
            .setTimeout(DEFAULT_MAX_DURATION)
            .setQuery(dbq);
        deleteByQueryRequest.setParentTask(parentTaskId);

        client.execute(DeleteByQueryAction.INSTANCE, deleteByQueryRequest, ActionListener.wrap(
            response -> {
                if (response.getBulkFailures().size() > 0 || response.getSearchFailures().size() > 0) {
                    LOGGER.error("Some unused stats documents could not be deleted due to failures: {}",
                        Strings.collectionToCommaDelimitedString(response.getBulkFailures()) +
                            "," + Strings.collectionToCommaDelimitedString(response.getSearchFailures()));
                } else {
                    LOGGER.info("Successfully deleted [{}] unused stats documents", response.getDeleted());
                }
                listener.onResponse(true);
            },
            e -> {
                LOGGER.error("Error deleting unused model stats documents: ", e);
                listener.onFailure(e);
            }
        ));
    }
}
