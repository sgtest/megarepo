/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.action;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchTimeoutException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.cluster.AckedClusterStateUpdateTask;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;

public abstract class AbstractTransportSetResetModeAction extends AcknowledgedTransportMasterNodeAction<SetResetModeActionRequest> {

    private static final Logger logger = LogManager.getLogger(AbstractTransportSetResetModeAction.class);
    private final ClusterService clusterService;

    @Inject
    public AbstractTransportSetResetModeAction(
        String actionName,
        TransportService transportService,
        ThreadPool threadPool,
        ClusterService clusterService,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver) {
        super(
            actionName,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            SetResetModeActionRequest::new,
            indexNameExpressionResolver,
            ThreadPool.Names.SAME
        );
        this.clusterService = clusterService;
    }

    protected abstract boolean isResetMode(ClusterState clusterState);

    protected abstract String featureName();

    protected abstract ClusterState setState(ClusterState oldState, SetResetModeActionRequest request);

    @Override
    protected void masterOperation(Task task,
                                   SetResetModeActionRequest request,
                                   ClusterState state,
                                   ActionListener<AcknowledgedResponse> listener) throws Exception {

        final boolean isResetModeEnabled = isResetMode(state);
        // Noop, nothing for us to do, simply return fast to the caller
        if (request.isEnabled() == isResetModeEnabled) {
            logger.debug(() -> new ParameterizedMessage("Reset mode noop for [{}]", featureName()));
            listener.onResponse(AcknowledgedResponse.TRUE);
            return;
        }

        logger.debug(
            () -> new ParameterizedMessage(
                "Starting to set [reset_mode] for [{}] to [{}] from [{}]",
                featureName(),
                request.isEnabled(),
                isResetModeEnabled
            )
        );

        ActionListener<AcknowledgedResponse> wrappedListener = ActionListener.wrap(
            r -> {
                logger.debug(() -> new ParameterizedMessage("Completed reset mode request for [{}]", featureName()));
                listener.onResponse(r);
            },
            e -> {
                logger.debug(
                    () -> new ParameterizedMessage("Completed reset mode for [{}] request but with failure", featureName()),
                    e
                );
                listener.onFailure(e);
            }
        );

        ActionListener<AcknowledgedResponse> clusterStateUpdateListener = ActionListener.wrap(
            acknowledgedResponse -> {
                if (acknowledgedResponse.isAcknowledged() == false) {
                    wrappedListener.onFailure(new ElasticsearchTimeoutException("Unknown error occurred while updating cluster state"));
                    return;
                }
                wrappedListener.onResponse(acknowledgedResponse);
            },
            wrappedListener::onFailure
        );

        clusterService.submitStateUpdateTask(featureName() + "-set-reset-mode",
            new AckedClusterStateUpdateTask(request, clusterStateUpdateListener) {

                @Override
                protected AcknowledgedResponse newResponse(boolean acknowledged) {
                    logger.trace(() -> new ParameterizedMessage("Cluster update response built for [{}]: {}", featureName(), acknowledged));
                    return AcknowledgedResponse.of(acknowledged);
                }

                @Override
                public ClusterState execute(ClusterState currentState) {
                    logger.trace(() -> new ParameterizedMessage("Executing cluster state update for [{}]", featureName()));
                    return setState(currentState, request);
                }
            });
    }

    @Override
    protected ClusterBlockException checkBlock(SetResetModeActionRequest request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

}
