/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.transform.action;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.lucene.util.SetOnce;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexAction;
import org.elasticsearch.action.admin.indices.delete.DeleteIndexRequest;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.master.AcknowledgedTransportMasterNodeAction;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.core.security.SecurityContext;
import org.elasticsearch.xpack.core.transform.action.ResetTransformAction;
import org.elasticsearch.xpack.core.transform.action.ResetTransformAction.Request;
import org.elasticsearch.xpack.core.transform.action.StopTransformAction;
import org.elasticsearch.xpack.core.transform.transforms.TransformConfig;
import org.elasticsearch.xpack.core.transform.transforms.TransformConfigUpdate;
import org.elasticsearch.xpack.transform.TransformServices;
import org.elasticsearch.xpack.transform.notifications.TransformAuditor;
import org.elasticsearch.xpack.transform.persistence.SeqNoPrimaryTermAndIndex;
import org.elasticsearch.xpack.transform.persistence.TransformConfigManager;
import org.elasticsearch.xpack.transform.persistence.TransformIndex;
import org.elasticsearch.xpack.transform.transforms.TransformTask;

import java.util.Objects;

import static org.elasticsearch.xpack.core.ClientHelper.TRANSFORM_ORIGIN;
import static org.elasticsearch.xpack.core.ClientHelper.executeAsyncWithOrigin;

public class TransportResetTransformAction extends AcknowledgedTransportMasterNodeAction<Request> {

    private static final Logger logger = LogManager.getLogger(TransportResetTransformAction.class);

    private final TransformConfigManager transformConfigManager;
    private final TransformAuditor auditor;
    private final Client client;
    private final SecurityContext securityContext;
    private final Settings settings;

    @Inject
    public TransportResetTransformAction(
        TransportService transportService,
        ClusterService clusterService,
        ThreadPool threadPool,
        ActionFilters actionFilters,
        IndexNameExpressionResolver indexNameExpressionResolver,
        TransformServices transformServices,
        Client client,
        Settings settings
    ) {
        super(
            ResetTransformAction.NAME,
            transportService,
            clusterService,
            threadPool,
            actionFilters,
            Request::new,
            indexNameExpressionResolver,
            ThreadPool.Names.SAME
        );
        this.transformConfigManager = transformServices.getConfigManager();
        this.auditor = transformServices.getAuditor();
        this.client = Objects.requireNonNull(client);
        this.securityContext = XPackSettings.SECURITY_ENABLED.get(settings)
            ? new SecurityContext(settings, threadPool.getThreadContext())
            : null;
        this.settings = settings;
    }

    @Override
    protected void masterOperation(Task task, Request request, ClusterState state, ActionListener<AcknowledgedResponse> listener) {
        final boolean transformIsRunning = TransformTask.getTransformTask(request.getId(), state) != null;
        if (transformIsRunning && request.isForce() == false) {
            listener.onFailure(
                new ElasticsearchStatusException(
                    "Cannot reset transform [" + request.getId() + "] as the task is running. Stop the task first",
                    RestStatus.CONFLICT
                )
            );
            return;
        }

        final SetOnce<Tuple<TransformConfig, SeqNoPrimaryTermAndIndex>> transformConfigAndVersionHolder = new SetOnce<>();

        // <6> Reset transform
        ActionListener<TransformUpdater.UpdateResult> updateTransformListener = ActionListener.wrap(
            unusedUpdateResult -> transformConfigManager.resetTransform(request.getId(), ActionListener.wrap(resetResponse -> {
                logger.debug("[{}] reset transform", request.getId());
                auditor.info(request.getId(), "Reset transform.");
                listener.onResponse(AcknowledgedResponse.of(resetResponse));
            }, listener::onFailure)),
            listener::onFailure
        );

        // <5> Upgrade transform to the latest version
        ActionListener<AcknowledgedResponse> deleteDestIndexListener = ActionListener.wrap(unusedDeleteDestIndexResponse -> {
            final ClusterState clusterState = clusterService.state();
            TransformUpdater.updateTransform(
                securityContext,
                indexNameExpressionResolver,
                clusterState,
                settings,
                client,
                transformConfigManager,
                transformConfigAndVersionHolder.get().v1(),
                TransformConfigUpdate.EMPTY,
                transformConfigAndVersionHolder.get().v2(),
                false, // defer validation
                false, // dry run
                false, // check access
                request.timeout(),
                updateTransformListener
            );
        }, listener::onFailure);

        // <4> Delete destination index if it was created by transform.
        ActionListener<Boolean> isDestinationIndexCreatedByTransformListener = ActionListener.wrap(isDestinationIndexCreatedByTransform -> {
            if (isDestinationIndexCreatedByTransform == false) {
                // Destination index was created outside of transform, we don't delete it and just move on.
                deleteDestIndexListener.onResponse(AcknowledgedResponse.TRUE);
                return;
            }
            String destIndex = transformConfigAndVersionHolder.get().v1().getDestination().getIndex();
            DeleteIndexRequest deleteDestIndexRequest = new DeleteIndexRequest(destIndex);
            executeAsyncWithOrigin(client, TRANSFORM_ORIGIN, DeleteIndexAction.INSTANCE, deleteDestIndexRequest, deleteDestIndexListener);
        }, listener::onFailure);

        // <3> Check if the destination index was created by transform
        ActionListener<Tuple<TransformConfig, SeqNoPrimaryTermAndIndex>> getTransformConfigurationListener = ActionListener.wrap(
            transformConfigAndVersion -> {
                transformConfigAndVersionHolder.set(transformConfigAndVersion);
                String destIndex = transformConfigAndVersion.v1().getDestination().getIndex();
                TransformIndex.isDestinationIndexCreatedByTransform(client, destIndex, isDestinationIndexCreatedByTransformListener);
            },
            listener::onFailure
        );

        // <2> Fetch transform configuration
        ActionListener<StopTransformAction.Response> stopTransformActionListener = ActionListener.wrap(
            unusedStopResponse -> transformConfigManager.getTransformConfigurationForUpdate(
                request.getId(),
                getTransformConfigurationListener
            ),
            listener::onFailure
        );

        // <1> Stop transform if it's currently running
        if (transformIsRunning == false) {
            stopTransformActionListener.onResponse(null);
            return;
        }
        StopTransformAction.Request stopTransformRequest = new StopTransformAction.Request(request.getId(), true, false, null, true, false);
        executeAsyncWithOrigin(client, TRANSFORM_ORIGIN, StopTransformAction.INSTANCE, stopTransformRequest, stopTransformActionListener);
    }

    @Override
    protected ClusterBlockException checkBlock(Request request, ClusterState state) {
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_READ);
    }
}
