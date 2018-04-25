/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.job.retention;

import org.apache.logging.log4j.Logger;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.xpack.core.ml.action.DeleteModelSnapshotAction;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.core.ml.job.persistence.AnomalyDetectorsIndex;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshot;
import org.elasticsearch.xpack.core.ml.job.process.autodetect.state.ModelSnapshotField;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.Objects;

/**
 * Deletes all model snapshots that have expired the configured retention time
 * of their respective job with the exception of the currently used snapshot.
 * A snapshot is deleted if its timestamp is earlier than the start of the
 * current day (local time-zone) minus the retention period.
 */
public class ExpiredModelSnapshotsRemover extends AbstractExpiredJobDataRemover {

    private static final Logger LOGGER = Loggers.getLogger(ExpiredModelSnapshotsRemover.class);

    /**
     *  The max number of snapshots to fetch per job. It is set to 10K, the default for an index as
     *  we don't change that in our ML indices. It should be more than enough for most cases. If not,
     *  it will take a few iterations to delete all snapshots, which is OK.
     */
    private static final int MODEL_SNAPSHOT_SEARCH_SIZE = 10000;

    private final Client client;

    public ExpiredModelSnapshotsRemover(Client client, ClusterService clusterService) {
        super(clusterService);
        this.client = Objects.requireNonNull(client);
    }

    @Override
    protected Long getRetentionDays(Job job) {
        return job.getModelSnapshotRetentionDays();
    }

    @Override
    protected void removeDataBefore(Job job, long cutoffEpochMs, ActionListener<Boolean> listener) {
        if (job.getModelSnapshotId() == null) {
            // No snapshot to remove
            listener.onResponse(true);
            return;
        }
        LOGGER.debug("Removing model snapshots of job [{}] that have a timestamp before [{}]", job.getId(), cutoffEpochMs);

        SearchRequest searchRequest = new SearchRequest();
        searchRequest.indices(AnomalyDetectorsIndex.jobResultsAliasedName(job.getId()));

        QueryBuilder activeSnapshotFilter = QueryBuilders.termQuery(
                ModelSnapshotField.SNAPSHOT_ID.getPreferredName(), job.getModelSnapshotId());
        QueryBuilder retainFilter = QueryBuilders.termQuery(ModelSnapshot.RETAIN.getPreferredName(), true);
        QueryBuilder query = createQuery(job.getId(), cutoffEpochMs)
                .filter(QueryBuilders.existsQuery(ModelSnapshot.SNAPSHOT_DOC_COUNT.getPreferredName()))
                .mustNot(activeSnapshotFilter)
                .mustNot(retainFilter);

        searchRequest.source(new SearchSourceBuilder().query(query).size(MODEL_SNAPSHOT_SEARCH_SIZE));

        client.execute(SearchAction.INSTANCE, searchRequest, new ActionListener<SearchResponse>() {
            @Override
            public void onResponse(SearchResponse searchResponse) {
                try {
                    List<ModelSnapshot> modelSnapshots = new ArrayList<>();
                    for (SearchHit hit : searchResponse.getHits()) {
                        modelSnapshots.add(ModelSnapshot.fromJson(hit.getSourceRef()));
                    }
                    deleteModelSnapshots(createVolatileCursorIterator(modelSnapshots), listener);
                } catch (Exception e) {
                    onFailure(e);
                }
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(new ElasticsearchException("[" + job.getId() +  "] Search for expired snapshots failed", e));
            }
        });
    }

    private void deleteModelSnapshots(Iterator<ModelSnapshot> modelSnapshotIterator, ActionListener<Boolean> listener) {
        if (modelSnapshotIterator.hasNext() == false) {
            listener.onResponse(true);
            return;
        }
        ModelSnapshot modelSnapshot = modelSnapshotIterator.next();
        DeleteModelSnapshotAction.Request deleteSnapshotRequest = new DeleteModelSnapshotAction.Request(
                modelSnapshot.getJobId(), modelSnapshot.getSnapshotId());
        client.execute(DeleteModelSnapshotAction.INSTANCE, deleteSnapshotRequest, new ActionListener<DeleteModelSnapshotAction.Response>() {
                @Override
                public void onResponse(DeleteModelSnapshotAction.Response response) {
                    try {
                        deleteModelSnapshots(modelSnapshotIterator, listener);
                    } catch (Exception e) {
                        onFailure(e);
                    }
                }

                @Override
                public void onFailure(Exception e) {
                    listener.onFailure(new ElasticsearchException("[" + modelSnapshot.getJobId() +  "] Failed to delete snapshot ["
                            + modelSnapshot.getSnapshotId() + "]", e));
                }
            });
    }
}
