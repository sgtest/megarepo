/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.upgrades;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.service.ClusterService;

import java.util.HashMap;

/**
 * Handles updating the {@link FeatureMigrationResults} in the cluster state.
 */
public class MigrationResultsUpdateTask extends ClusterStateUpdateTask {
    private static final Logger logger = LogManager.getLogger(MigrationResultsUpdateTask.class);

    private final String featureName;
    private final SingleFeatureMigrationResult status;
    private final ActionListener<ClusterState> listener;

    private MigrationResultsUpdateTask(String featureName, SingleFeatureMigrationResult status, ActionListener<ClusterState> listener) {
        this.featureName = featureName;
        this.status = status;
        this.listener = listener;
    }

    /**
     * Creates a task that will update the status of a feature migration.
     * @param featureName The name of the feature whose status should be updated.
     * @param status The status to be associated with the given feature.
     * @param listener A listener that will be called upon successfully updating the cluster state.
     */
    public static MigrationResultsUpdateTask upsert(
        String featureName,
        SingleFeatureMigrationResult status,
        ActionListener<ClusterState> listener
    ) {
        return new MigrationResultsUpdateTask(featureName, status, listener);
    }

    /**
     * Submit the update task so that it will actually be executed.
     * @param clusterService The cluster service to which this task should be submitted.
     */
    public void submit(ClusterService clusterService) {
        String source = new ParameterizedMessage("record [{}] migration [{}]", featureName, status.succeeded() ? "success" : "failure")
            .getFormattedMessage();
        clusterService.submitStateUpdateTask(source, this);
    }

    @Override
    public ClusterState execute(ClusterState currentState) throws Exception {
        FeatureMigrationResults currentResults = currentState.metadata().custom(FeatureMigrationResults.TYPE);
        if (currentResults == null) {
            currentResults = new FeatureMigrationResults(new HashMap<>());
        }
        FeatureMigrationResults newResults = currentResults.withResult(featureName, status);
        final Metadata newMetadata = Metadata.builder(currentState.metadata())
            .putCustom(FeatureMigrationResults.TYPE, newResults)
            .build();
        final ClusterState newState = ClusterState.builder(currentState).metadata(newMetadata).build();
        return newState;
    }

    @Override
    public void clusterStateProcessed(String source, ClusterState oldState, ClusterState newState) {
        listener.onResponse(newState);
    }

    @Override
    public void onFailure(String source, Exception clusterStateUpdateException) {
        if (status.succeeded()) {
            logger.warn(
                new ParameterizedMessage("failed to update cluster state after successful migration of feature [{}]", featureName),
                clusterStateUpdateException
            );
        } else {
            logger.error(
                new ParameterizedMessage(
                    "failed to update cluster state after failed migration of feature [{}] on index [{}]",
                    featureName,
                    status.getFailedIndexName()
                ).getFormattedMessage(),
                clusterStateUpdateException
            );
        }
        listener.onFailure(clusterStateUpdateException);
    }
}
