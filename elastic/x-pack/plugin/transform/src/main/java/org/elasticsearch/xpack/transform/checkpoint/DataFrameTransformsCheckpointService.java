/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.transform.checkpoint;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.xpack.core.transform.transforms.TransformIndexerPosition;
import org.elasticsearch.xpack.core.transform.transforms.TransformCheckpointingInfo;
import org.elasticsearch.xpack.core.transform.transforms.TransformConfig;
import org.elasticsearch.xpack.core.transform.transforms.TransformProgress;
import org.elasticsearch.xpack.core.transform.transforms.TimeSyncConfig;
import org.elasticsearch.xpack.transform.notifications.DataFrameAuditor;
import org.elasticsearch.xpack.transform.persistence.DataFrameTransformsConfigManager;

/**
 * DataFrameTransform Checkpoint Service
 *
 * Allows checkpointing a source of a data frame transform which includes all relevant checkpoints of the source.
 *
 * This will be used to checkpoint a transform, detect changes, run the transform in continuous mode.
 *
 */
public class DataFrameTransformsCheckpointService {

    private static final Logger logger = LogManager.getLogger(DataFrameTransformsCheckpointService.class);

    private final Client client;
    private final DataFrameTransformsConfigManager dataFrameTransformsConfigManager;
    private final DataFrameAuditor dataFrameAuditor;

    public DataFrameTransformsCheckpointService(final Client client,
            final DataFrameTransformsConfigManager dataFrameTransformsConfigManager, DataFrameAuditor dataFrameAuditor) {
        this.client = client;
        this.dataFrameTransformsConfigManager = dataFrameTransformsConfigManager;
        this.dataFrameAuditor = dataFrameAuditor;
    }

    public CheckpointProvider getCheckpointProvider(final TransformConfig transformConfig) {
        if (transformConfig.getSyncConfig() instanceof TimeSyncConfig) {
            return new TimeBasedCheckpointProvider(client, dataFrameTransformsConfigManager, dataFrameAuditor, transformConfig);
        }

        return new DefaultCheckpointProvider(client, dataFrameTransformsConfigManager, dataFrameAuditor, transformConfig);
    }

    /**
     * Get checkpointing stats for a stopped data frame
     *
     * @param transformId The data frame task
     * @param lastCheckpointNumber the last checkpoint
     * @param nextCheckpointPosition position for the next checkpoint
     * @param nextCheckpointProgress progress for the next checkpoint
     * @param listener listener to retrieve the result
     */
    public void getCheckpointingInfo(final String transformId,
                                     final long lastCheckpointNumber,
                                     final TransformIndexerPosition nextCheckpointPosition,
                                     final TransformProgress nextCheckpointProgress,
                                     final ActionListener<TransformCheckpointingInfo> listener) {

        // we need to retrieve the config first before we can defer the rest to the corresponding provider
        dataFrameTransformsConfigManager.getTransformConfiguration(transformId, ActionListener.wrap(
            transformConfig -> {
                getCheckpointProvider(transformConfig).getCheckpointingInfo(lastCheckpointNumber,
                            nextCheckpointPosition, nextCheckpointProgress, listener);
                },
            transformError -> {
                logger.warn("Failed to retrieve configuration for data frame [" + transformId + "]", transformError);
                listener.onFailure(new CheckpointException("Failed to retrieve configuration", transformError));
            })
        );
    }

}
