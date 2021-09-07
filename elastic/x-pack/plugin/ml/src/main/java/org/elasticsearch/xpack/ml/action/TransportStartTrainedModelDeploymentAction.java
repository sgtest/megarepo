/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.action;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.ActionFilters;
import org.elasticsearch.action.support.master.TransportMasterNodeAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.OriginSettingClient;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.block.ClusterBlockException;
import org.elasticsearch.cluster.block.ClusterBlockLevel;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.NodesShutdownMetadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.inject.Inject;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.license.LicenseUtils;
import org.elasticsearch.license.XPackLicenseState;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.tasks.Task;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.TransportService;
import org.elasticsearch.xpack.core.XPackField;
import org.elasticsearch.xpack.core.ml.action.CreateTrainedModelAllocationAction;
import org.elasticsearch.xpack.core.ml.action.GetTrainedModelsAction;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction.TaskParams;
import org.elasticsearch.xpack.core.ml.inference.TrainedModelConfig;
import org.elasticsearch.xpack.core.ml.inference.TrainedModelType;
import org.elasticsearch.xpack.core.ml.inference.allocation.AllocationStatus;
import org.elasticsearch.xpack.core.ml.inference.allocation.RoutingState;
import org.elasticsearch.xpack.core.ml.inference.allocation.RoutingStateAndReason;
import org.elasticsearch.xpack.core.ml.inference.allocation.TrainedModelAllocation;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.inference.allocation.TrainedModelAllocationMetadata;
import org.elasticsearch.xpack.ml.inference.allocation.TrainedModelAllocationService;
import org.elasticsearch.xpack.ml.inference.persistence.ChunkedTrainedModelRestorer;
import org.elasticsearch.xpack.ml.process.MlMemoryTracker;

import java.util.Collections;
import java.util.HashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.function.Predicate;
import java.util.stream.Collectors;

import static org.elasticsearch.xpack.core.ClientHelper.ML_ORIGIN;

public class TransportStartTrainedModelDeploymentAction
    extends TransportMasterNodeAction<StartTrainedModelDeploymentAction.Request, CreateTrainedModelAllocationAction.Response> {

    private static final Logger logger = LogManager.getLogger(TransportStartTrainedModelDeploymentAction.class);

    private final XPackLicenseState licenseState;
    private final Client client;
    private final TrainedModelAllocationService trainedModelAllocationService;
    private final NamedXContentRegistry xContentRegistry;
    private final MlMemoryTracker memoryTracker;

    @Inject
    public TransportStartTrainedModelDeploymentAction(TransportService transportService, Client client, ClusterService clusterService,
                                                      ThreadPool threadPool, ActionFilters actionFilters, XPackLicenseState licenseState,
                                                      IndexNameExpressionResolver indexNameExpressionResolver,
                                                      TrainedModelAllocationService trainedModelAllocationService,
                                                      NamedXContentRegistry xContentRegistry, MlMemoryTracker memoryTracker) {
        super(StartTrainedModelDeploymentAction.NAME, transportService, clusterService, threadPool, actionFilters,
            StartTrainedModelDeploymentAction.Request::new, indexNameExpressionResolver, CreateTrainedModelAllocationAction.Response::new,
            ThreadPool.Names.SAME);
        this.licenseState = Objects.requireNonNull(licenseState);
        this.client = new OriginSettingClient(Objects.requireNonNull(client), ML_ORIGIN);
        this.xContentRegistry = Objects.requireNonNull(xContentRegistry);
        this.memoryTracker = Objects.requireNonNull(memoryTracker);
        this.trainedModelAllocationService = Objects.requireNonNull(trainedModelAllocationService);
    }

    @Override
    protected void masterOperation(Task task, StartTrainedModelDeploymentAction.Request request, ClusterState state,
                                   ActionListener<CreateTrainedModelAllocationAction.Response> listener) throws Exception {
        logger.trace(() -> new ParameterizedMessage("[{}] received deploy request", request.getModelId()));
        if (licenseState.checkFeature(XPackLicenseState.Feature.MACHINE_LEARNING) == false) {
            listener.onFailure(LicenseUtils.newComplianceException(XPackField.MACHINE_LEARNING));
            return;
        }

        ActionListener<CreateTrainedModelAllocationAction.Response> waitForDeploymentToStart =
            ActionListener.wrap(
                modelAllocation -> waitForDeploymentState(
                    request.getModelId(),
                    request.getTimeout(),
                    request.getWaitForState(),
                    listener
                ),
                e -> {
                    logger.warn(() -> new ParameterizedMessage("[{}] creating new allocation failed", request.getModelId()), e);
                    if (ExceptionsHelper.unwrapCause(e) instanceof ResourceAlreadyExistsException) {
                        e = new ElasticsearchStatusException(
                            "Cannot start deployment [{}] because it has already been started",
                            RestStatus.CONFLICT,
                            e,
                            request.getModelId()
                        );
                    }
                    listener.onFailure(e);
                }
            );

        ActionListener<GetTrainedModelsAction.Response> getModelListener = ActionListener.wrap(
            getModelResponse -> {
                if (getModelResponse.getResources().results().size() > 1) {
                    listener.onFailure(ExceptionsHelper.badRequestException(
                        "cannot deploy more than one models at the same time; [{}] matches [{}] models]",
                        request.getModelId(), getModelResponse.getResources().results().size()));
                    return;
                }


                TrainedModelConfig trainedModelConfig = getModelResponse.getResources().results().get(0);
                if (trainedModelConfig.getModelType() != TrainedModelType.PYTORCH) {
                    listener.onFailure(ExceptionsHelper.badRequestException(
                        "model [{}] of type [{}] cannot be deployed. Only PyTorch models can be deployed",
                        trainedModelConfig.getModelId(), trainedModelConfig.getModelType()));
                    return;
                }

                if (trainedModelConfig.getLocation() == null) {
                    listener.onFailure(ExceptionsHelper.serverError(
                        "model [{}] does not have location", trainedModelConfig.getModelId()));
                    return;
                }

                getModelBytes(trainedModelConfig, ActionListener.wrap(
                    modelBytes -> {
                        TaskParams taskParams = new TaskParams(trainedModelConfig.getModelId(), modelBytes);
                        PersistentTasksCustomMetadata persistentTasks = clusterService.state().getMetadata().custom(
                            PersistentTasksCustomMetadata.TYPE);
                        memoryTracker.refresh(persistentTasks, ActionListener.wrap(
                                aVoid -> trainedModelAllocationService.createNewModelAllocation(
                                    taskParams,
                                    waitForDeploymentToStart
                                ),
                                listener::onFailure
                            )
                        );
                    },
                    listener::onFailure
                ));

            },
            listener::onFailure
        );

        GetTrainedModelsAction.Request getModelRequest = new GetTrainedModelsAction.Request(request.getModelId());
        client.execute(GetTrainedModelsAction.INSTANCE, getModelRequest, getModelListener);
    }

    private void getModelBytes(TrainedModelConfig trainedModelConfig, ActionListener<Long> listener) {
        ChunkedTrainedModelRestorer restorer = new ChunkedTrainedModelRestorer(trainedModelConfig.getModelId(),
            client, threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME), xContentRegistry);
        restorer.setSearchIndex(trainedModelConfig.getLocation().getResourceName());
        restorer.setSearchSize(1);
        restorer.restoreModelDefinition(
            doc -> {
                // The in-memory size of the model was found to be approximately equal
                // to the size of the model on disk in experiments for BERT models. However,
                // this might not always be the case.
                // TODO Improve heuristic for in-memory model size.
                listener.onResponse(doc.getTotalDefinitionLength());

                // Return false to stop the restorer as we only need the first doc
                return false;
            },
            success -> { /* nothing to do */ },
            listener::onFailure
        );
    }

    private void waitForDeploymentState(
        String modelId,
        TimeValue timeout,
        AllocationStatus.State state,
        ActionListener<CreateTrainedModelAllocationAction.Response> listener
    ) {
        DeploymentStartedPredicate predicate = new DeploymentStartedPredicate(modelId, state);
        trainedModelAllocationService.waitForAllocationCondition(modelId, predicate, timeout,
            new TrainedModelAllocationService.WaitForAllocationListener() {
                @Override
                public void onResponse(TrainedModelAllocation allocation) {
                    if (predicate.exception != null) {
                        deleteFailedDeployment(modelId, predicate.exception, listener);
                    } else {
                        listener.onResponse(new CreateTrainedModelAllocationAction.Response(allocation));
                    }
                }

                @Override
                public void onFailure(Exception e) {
                    listener.onFailure(e);
                }
            });
    }

    private void deleteFailedDeployment(
        String modelId,
        Exception exception,
        ActionListener<CreateTrainedModelAllocationAction.Response> listener
    ) {
        trainedModelAllocationService.deleteModelAllocation(modelId, ActionListener.wrap(
            pTask -> listener.onFailure(exception),
            e -> {
                logger.error(
                    new ParameterizedMessage(
                        "[{}] Failed to delete model allocation that had failed with the reason [{}]",
                        modelId,
                        exception.getMessage()
                    ),
                    e
                );
                listener.onFailure(exception);
            }
        ));

    }

    @Override
    protected ClusterBlockException checkBlock(StartTrainedModelDeploymentAction.Request request, ClusterState state) {
        // We only delegate here to PersistentTasksService, but if there is a metadata writeblock,
        // then delegating to PersistentTasksService doesn't make a whole lot of sense,
        // because PersistentTasksService will then fail.
        return state.blocks().globalBlockedException(ClusterBlockLevel.METADATA_WRITE);
    }

    private static class DeploymentStartedPredicate implements Predicate<ClusterState> {

        private volatile Exception exception;

        // for logging
        private final String modelId;
        private final AllocationStatus.State waitForState;

        DeploymentStartedPredicate(String modelId, AllocationStatus.State waitForState) {
            this.modelId = ExceptionsHelper.requireNonNull(modelId, "model_id");
            this.waitForState = waitForState;
        }

        @Override
        public boolean test(ClusterState clusterState) {
            TrainedModelAllocation trainedModelAllocation = TrainedModelAllocationMetadata.allocationForModelId(clusterState, modelId)
                .orElse(null);
            if (trainedModelAllocation == null) {
                // Something weird happened, it should NEVER be null...
                return true;
            }

            final Set<Map.Entry<String, RoutingStateAndReason>> nodesAndState = trainedModelAllocation
                .getNodeRoutingTable()
                .entrySet();

            Map<String, String> nodeFailuresAndReasons = new HashMap<>();
            Set<String> nodesStillInitializing = new LinkedHashSet<>();
            for (Map.Entry<String, RoutingStateAndReason> nodeIdAndState : nodesAndState) {
                if (RoutingState.FAILED.equals(nodeIdAndState.getValue().getState())) {
                    nodeFailuresAndReasons.put(nodeIdAndState.getKey(), nodeIdAndState.getValue().getReason());
                }
                if (RoutingState.STARTING.equals(nodeIdAndState.getValue().getState())) {
                    nodesStillInitializing.add(nodeIdAndState.getKey());
                }
            }

            if (nodeFailuresAndReasons.isEmpty() == false) {
                exception = new ElasticsearchStatusException(
                    "Could not start trained model deployment, the following nodes failed with errors [{}]",
                    RestStatus.INTERNAL_SERVER_ERROR,
                    nodeFailuresAndReasons
                );
                return true;
            }

            // No nodes allocated at all!
            // TODO when we support autoscaling for this, check for `maxLazyNodes` setting
            if (nodesAndState.isEmpty()) {
                String msg = "Could not start deployment because no suitable nodes were found, allocation explanation ["
                    + trainedModelAllocation.getReason()
                    + "]";
                logger.warn("[{}] {}", modelId, msg);
                Exception detail = new IllegalStateException(msg);
                exception = new ElasticsearchStatusException(
                    "Could not start deployment because no ML nodes with sufficient capacity were found",
                    RestStatus.TOO_MANY_REQUESTS,
                    detail
                );
                return true;
            }

            Set<String> nodesShuttingDown = nodesShuttingDown(clusterState);
            List<DiscoveryNode> nodes = clusterState.nodes()
                .getAllNodes()
                .stream()
                .filter(d -> nodesShuttingDown.contains(d.getId()) == false)
                .filter(TaskParams::mayAllocateToNode)
                .collect(Collectors.toList());
            AllocationStatus allocationStatus = trainedModelAllocation.calculateAllocationStatus(nodes).orElse(null);
            if (allocationStatus == null || allocationStatus.calculateState().compareTo(waitForState) >= 0) {
                return true;
            }

            if (nodesStillInitializing.isEmpty()) {
                return true;
            }
            logger.trace(
                () -> new ParameterizedMessage(
                    "[{}] tested with state [{}] and nodes {} still initializing",
                    modelId,
                    trainedModelAllocation.getAllocationState(),
                    nodesStillInitializing
                )
            );
            return false;
        }
    }

    static Set<String> nodesShuttingDown(final ClusterState state) {
        return NodesShutdownMetadata.getShutdowns(state)
            .map(NodesShutdownMetadata::getAllNodeMetadataMap)
            .map(Map::keySet)
            .orElse(Collections.emptySet());
    }

}
