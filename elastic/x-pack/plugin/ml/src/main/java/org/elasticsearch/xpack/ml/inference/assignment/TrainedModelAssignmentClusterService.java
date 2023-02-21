/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.assignment;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.ClusterStateListener;
import org.elasticsearch.cluster.ClusterStateUpdateTask;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.NodesShutdownMetadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.core.SuppressForbidden;
import org.elasticsearch.gateway.GatewayService;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.ml.MlTasks;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.action.UpdateTrainedModelAssignmentRoutingInfoAction;
import org.elasticsearch.xpack.core.ml.inference.assignment.AssignmentState;
import org.elasticsearch.xpack.core.ml.inference.assignment.RoutingInfo;
import org.elasticsearch.xpack.core.ml.inference.assignment.RoutingState;
import org.elasticsearch.xpack.core.ml.inference.assignment.TrainedModelAssignment;
import org.elasticsearch.xpack.core.ml.job.messages.Messages;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.autoscaling.NodeAvailabilityZoneMapper;
import org.elasticsearch.xpack.ml.inference.assignment.planning.AllocationReducer;
import org.elasticsearch.xpack.ml.job.NodeLoad;
import org.elasticsearch.xpack.ml.job.NodeLoadDetector;
import org.elasticsearch.xpack.ml.notifications.SystemAuditor;

import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.OptionalLong;
import java.util.Set;
import java.util.function.Function;
import java.util.stream.Collectors;

import static org.elasticsearch.core.Strings.format;

public class TrainedModelAssignmentClusterService implements ClusterStateListener {

    private static final Logger logger = LogManager.getLogger(TrainedModelAssignmentClusterService.class);

    private static final Version RENAME_ALLOCATION_TO_ASSIGNMENT_VERSION = Version.V_8_3_0;
    public static final Version DISTRIBUTED_MODEL_ALLOCATION_VERSION = Version.V_8_4_0;

    private final ClusterService clusterService;
    private final ThreadPool threadPool;
    private final NodeLoadDetector nodeLoadDetector;
    private final SystemAuditor systemAuditor;
    private final NodeAvailabilityZoneMapper nodeAvailabilityZoneMapper;
    private volatile int maxMemoryPercentage;
    private volatile boolean useAuto;
    private volatile int maxOpenJobs;
    protected volatile int maxLazyMLNodes;
    protected volatile long maxMLNodeSize;

    public TrainedModelAssignmentClusterService(
        Settings settings,
        ClusterService clusterService,
        ThreadPool threadPool,
        NodeLoadDetector nodeLoadDetector,
        SystemAuditor systemAuditor,
        NodeAvailabilityZoneMapper nodeAvailabilityZoneMapper
    ) {
        this.clusterService = Objects.requireNonNull(clusterService);
        this.threadPool = Objects.requireNonNull(threadPool);
        this.nodeLoadDetector = Objects.requireNonNull(nodeLoadDetector);
        this.systemAuditor = Objects.requireNonNull(systemAuditor);
        this.nodeAvailabilityZoneMapper = Objects.requireNonNull(nodeAvailabilityZoneMapper);
        this.maxMemoryPercentage = MachineLearning.MAX_MACHINE_MEMORY_PERCENT.get(settings);
        this.useAuto = MachineLearning.USE_AUTO_MACHINE_MEMORY_PERCENT.get(settings);
        this.maxOpenJobs = MachineLearning.MAX_OPEN_JOBS_PER_NODE.get(settings);
        this.maxLazyMLNodes = MachineLearning.MAX_LAZY_ML_NODES.get(settings);
        this.maxMLNodeSize = MachineLearning.MAX_ML_NODE_SIZE.get(settings).getBytes();
        // Only nodes that can possibly be master nodes really need this service running
        if (DiscoveryNode.isMasterNode(settings)) {
            clusterService.addListener(this);
            clusterService.getClusterSettings()
                .addSettingsUpdateConsumer(MachineLearning.MAX_MACHINE_MEMORY_PERCENT, this::setMaxMemoryPercentage);
            clusterService.getClusterSettings()
                .addSettingsUpdateConsumer(MachineLearning.USE_AUTO_MACHINE_MEMORY_PERCENT, this::setUseAuto);
            clusterService.getClusterSettings().addSettingsUpdateConsumer(MachineLearning.MAX_OPEN_JOBS_PER_NODE, this::setMaxOpenJobs);
            clusterService.getClusterSettings().addSettingsUpdateConsumer(MachineLearning.MAX_LAZY_ML_NODES, this::setMaxLazyMLNodes);
            clusterService.getClusterSettings().addSettingsUpdateConsumer(MachineLearning.MAX_ML_NODE_SIZE, this::setMaxMLNodeSize);
        }
    }

    private void setMaxMemoryPercentage(int maxMemoryPercentage) {
        this.maxMemoryPercentage = maxMemoryPercentage;
    }

    private void setUseAuto(boolean useAuto) {
        this.useAuto = useAuto;
    }

    private void setMaxOpenJobs(int maxOpenJobs) {
        this.maxOpenJobs = maxOpenJobs;
    }

    private void setMaxLazyMLNodes(int value) {
        this.maxLazyMLNodes = value;
    }

    private void setMaxMLNodeSize(ByteSizeValue value) {
        this.maxMLNodeSize = value.getBytes();
    }

    @SuppressForbidden(reason = "legacy usage of unbatched task") // TODO add support for batching here
    private void submitUnbatchedTask(@SuppressWarnings("SameParameterValue") String source, ClusterStateUpdateTask task) {
        clusterService.submitUnbatchedStateUpdateTask(source, task);
    }

    @Override
    public void clusterChanged(ClusterChangedEvent event) {
        if (event.state().blocks().hasGlobalBlock(GatewayService.STATE_NOT_RECOVERED_BLOCK)) {
            return;
        }
        if (event.localNodeMaster() == false) {
            return;
        }

        if (event.state().nodes().getMinNodeVersion().before(DISTRIBUTED_MODEL_ALLOCATION_VERSION)) {
            // we should not try to rebalance assignments while there may be nodes running on a version
            // prior to introducing distributed model allocation.
            // But we should remove routing to removed or shutting down nodes.
            removeRoutingToRemovedOrShuttingDownNodes(event);
            return;
        }

        Optional<String> rebalanceReason = detectReasonToRebalanceModels(event);
        if (rebalanceReason.isPresent()) {
            // As this produces a cluster state update task, we are certain that if the persistent
            // task framework results in assigning some ML tasks on that same cluster state change
            // we do not end up over-allocating a node. Both this service and the persistent task service
            // will produce a cluster state update but the one that gets applied first wins. The other
            // update will be rejected and we will retry to assign getting a correct update on available memory
            // on each node.
            rebalanceAssignments(
                event.state(),
                Optional.empty(),
                rebalanceReason.get(),
                ActionListener.wrap(
                    newMetadata -> logger.debug(
                        () -> format("rebalanced model assignments [%s]", Strings.toString(newMetadata, false, true))
                    ),
                    e -> logger.warn("failed to rebalance models", e)
                )
            );
        }
    }

    private void removeRoutingToRemovedOrShuttingDownNodes(ClusterChangedEvent event) {
        if (areAssignedNodesRemoved(event)) {
            submitUnbatchedTask("removing routing entries for removed or shutting down nodes", new ClusterStateUpdateTask() {
                @Override
                public ClusterState execute(ClusterState currentState) {
                    return removeRoutingToUnassignableNodes(currentState);
                }

                @Override
                public void onFailure(Exception e) {
                    logger.error("could not remove routing entries for removed or shutting down nodes", e);
                }

                @Override
                public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                    logger.debug(
                        () -> format(
                            "updated model assignments based on node changes in the cluster; new metadata [%s]",
                            Strings.toString(TrainedModelAssignmentMetadata.fromState(newState), false, true)
                        )
                    );
                }
            });
        }
    }

    // Visible for testing
    static boolean areAssignedNodesRemoved(ClusterChangedEvent event) {
        boolean nodesShutdownChanged = event.changedCustomMetadataSet().contains(NodesShutdownMetadata.TYPE);
        if (event.nodesRemoved() || nodesShutdownChanged) {
            Set<String> removedOrShuttingDownNodeIds = new HashSet<>(nodesShuttingDown(event.state()));
            event.nodesDelta().removedNodes().stream().map(DiscoveryNode::getId).forEach(removedOrShuttingDownNodeIds::add);

            TrainedModelAssignmentMetadata metadata = TrainedModelAssignmentMetadata.fromState(event.state());
            for (TrainedModelAssignment assignment : metadata.modelAssignments().values()) {
                if (Sets.intersection(removedOrShuttingDownNodeIds, assignment.getNodeRoutingTable().keySet()).isEmpty() == false) {
                    return true;
                }
            }
        }
        return false;
    }

    // Visible for testing
    static ClusterState removeRoutingToUnassignableNodes(ClusterState currentState) {
        Set<String> assignableNodes = getAssignableNodes(currentState).stream().map(DiscoveryNode::getId).collect(Collectors.toSet());
        TrainedModelAssignmentMetadata metadata = TrainedModelAssignmentMetadata.fromState(currentState);
        TrainedModelAssignmentMetadata.Builder builder = TrainedModelAssignmentMetadata.builder(currentState);
        for (TrainedModelAssignment assignment : metadata.modelAssignments().values()) {
            Set<String> routedNodeIdsToRemove = Sets.difference(assignment.getNodeRoutingTable().keySet(), assignableNodes);
            if (routedNodeIdsToRemove.isEmpty() == false) {
                logger.debug(
                    () -> format(
                        "[%s] removing routing entries to nodes %s because they have been removed or are shutting down",
                        assignment.getModelId(),
                        routedNodeIdsToRemove
                    )
                );
                TrainedModelAssignment.Builder assignmentBuilder = TrainedModelAssignment.Builder.fromAssignment(assignment);
                routedNodeIdsToRemove.forEach(assignmentBuilder::removeRoutingEntry);
                builder.updateAssignment(assignment.getModelId(), assignmentBuilder.calculateAndSetAssignmentState());
            }
        }
        return update(currentState, builder);
    }

    public void updateModelRoutingTable(
        UpdateTrainedModelAssignmentRoutingInfoAction.Request request,
        ActionListener<AcknowledgedResponse> listener
    ) {
        logger.debug(
            () -> format(
                "[%s] updating routing table entry for node [%s], update [%s]",
                request.getModelId(),
                request.getNodeId(),
                request.getUpdate()
            )
        );
        submitUnbatchedTask("updating model routing for node assignment", new ClusterStateUpdateTask() {
            @Override
            public ClusterState execute(ClusterState currentState) {
                return updateModelRoutingTable(currentState, request);
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
            }

            @Override
            public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                listener.onResponse(AcknowledgedResponse.TRUE);
            }
        });
    }

    public void createNewModelAssignment(
        StartTrainedModelDeploymentAction.TaskParams params,
        ActionListener<TrainedModelAssignment> listener
    ) {
        if (clusterService.state().nodes().getMinNodeVersion().before(DISTRIBUTED_MODEL_ALLOCATION_VERSION)) {
            listener.onFailure(
                new ElasticsearchStatusException(
                    "cannot create new assignment for model [{}] while there are nodes older than version [{}]",
                    RestStatus.CONFLICT,
                    params.getModelId(),
                    DISTRIBUTED_MODEL_ALLOCATION_VERSION
                )
            );
            return;
        }

        if (MlMetadata.getMlMetadata(clusterService.state()).isResetMode()) {
            listener.onFailure(
                new ElasticsearchStatusException(
                    "cannot create new assignment for model [{}] while feature reset is in progress.",
                    RestStatus.CONFLICT,
                    params.getModelId()
                )
            );
            return;
        }

        rebalanceAssignments(clusterService.state(), Optional.of(params), "model deployment started", ActionListener.wrap(newMetadata -> {
            TrainedModelAssignment assignment = newMetadata.getModelAssignment(params.getModelId());
            if (assignment == null) {
                // If we could not allocate the model anywhere then it is possible the assignment
                // here is null. We should notify the listener of an empty assignment as the
                // handling of this is done elsewhere with the wait-to-start predicate.
                assignment = TrainedModelAssignment.Builder.empty(params).build();
            }
            listener.onResponse(assignment);
        }, listener::onFailure));
    }

    public void setModelAssignmentToStopping(String modelId, ActionListener<AcknowledgedResponse> listener) {
        submitUnbatchedTask("set model assignment stopping", new ClusterStateUpdateTask() {
            @Override
            public ClusterState execute(ClusterState currentState) {
                return setToStopping(currentState, modelId, "client API call");
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
            }

            @Override
            public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                listener.onResponse(AcknowledgedResponse.TRUE);
            }
        });
    }

    public void removeModelAssignment(String modelId, ActionListener<AcknowledgedResponse> listener) {
        submitUnbatchedTask("delete model assignment", new ClusterStateUpdateTask() {
            @Override
            public ClusterState execute(ClusterState currentState) {
                return removeAssignment(currentState, modelId);
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
            }

            @Override
            public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                // As a model deployment has been stopped we should rebalance as we might now
                // be able to satisfy more allocations for the rest of the deployments.
                rebalanceAssignments(
                    newState,
                    Optional.empty(),
                    "model deployment stopped",
                    ActionListener.wrap(
                        metadataAfterRebalance -> logger.debug(
                            () -> format("Successfully rebalanced model deployments after deployment for model [%s] was stopped", modelId)
                        ),
                        e -> logger.error(
                            format("Failed to rebalance model deployments after deployment for model [%s] was stopped", modelId),
                            e
                        )
                    )
                );
                listener.onResponse(AcknowledgedResponse.TRUE);
            }
        });
    }

    // Used by the reset action directly
    public void removeAllModelAssignments(ActionListener<AcknowledgedResponse> listener) {
        submitUnbatchedTask("delete all model assignments", new ClusterStateUpdateTask() {
            @Override
            public ClusterState execute(ClusterState currentState) {
                return removeAllAssignments(currentState);
            }

            @Override
            public void onFailure(Exception e) {
                listener.onFailure(e);
            }

            @Override
            public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                listener.onResponse(AcknowledgedResponse.TRUE);
            }
        });
    }

    private static ClusterState update(ClusterState currentState, TrainedModelAssignmentMetadata.Builder modelAssignments) {
        TrainedModelAssignmentMetadata previousMetadata = TrainedModelAssignmentMetadata.fromState(currentState);
        TrainedModelAssignmentMetadata updatedMetadata = modelAssignments.build();
        if (updatedMetadata.equals(previousMetadata)) {
            return currentState;
        } else {
            return forceUpdate(currentState, modelAssignments);
        }
    }

    private static ClusterState forceUpdate(ClusterState currentState, TrainedModelAssignmentMetadata.Builder modelAssignments) {
        logger.debug(() -> format("updated assignments: %s", modelAssignments.build()));
        Metadata.Builder metadata = Metadata.builder(currentState.metadata());
        if (currentState.getNodes().getMinNodeVersion().onOrAfter(RENAME_ALLOCATION_TO_ASSIGNMENT_VERSION)) {
            metadata.putCustom(TrainedModelAssignmentMetadata.NAME, modelAssignments.build())
                .removeCustom(TrainedModelAssignmentMetadata.DEPRECATED_NAME);
        } else {
            metadata.putCustom(TrainedModelAssignmentMetadata.DEPRECATED_NAME, modelAssignments.buildOld());
        }
        return ClusterState.builder(currentState).metadata(metadata).build();
    }

    ClusterState createModelAssignment(ClusterState currentState, StartTrainedModelDeploymentAction.TaskParams params) throws Exception {
        return update(currentState, rebalanceAssignments(currentState, Optional.of(params)));
    }

    private void rebalanceAssignments(
        ClusterState clusterState,
        Optional<StartTrainedModelDeploymentAction.TaskParams> modelToAdd,
        String reason,
        ActionListener<TrainedModelAssignmentMetadata> listener
    ) {
        threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME).execute(() -> {
            logger.debug(() -> format("Rebalancing model allocations because [%s]", reason));
            TrainedModelAssignmentMetadata.Builder rebalancedMetadata;
            try {
                rebalancedMetadata = rebalanceAssignments(clusterState, modelToAdd);
            } catch (Exception e) {
                listener.onFailure(e);
                return;
            }

            submitUnbatchedTask(reason, new ClusterStateUpdateTask() {

                private volatile boolean isUpdated;
                private volatile boolean isChanged;

                @Override
                public ClusterState execute(ClusterState currentState) {

                    if (areClusterStatesCompatibleForRebalance(clusterState, currentState)) {
                        isUpdated = true;
                        ClusterState updatedState = update(currentState, rebalancedMetadata);
                        isChanged = updatedState != currentState;
                        return updatedState;
                    }
                    rebalanceAssignments(currentState, modelToAdd, reason, listener);
                    return currentState;
                }

                @Override
                public void onFailure(Exception e) {
                    listener.onFailure(e);
                }

                @Override
                public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                    if (isUpdated) {
                        if (isChanged) {
                            threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME)
                                .execute(() -> systemAuditor.info(Messages.getMessage(Messages.INFERENCE_DEPLOYMENT_REBALANCED, reason)));
                        }
                        listener.onResponse(TrainedModelAssignmentMetadata.fromState(newState));
                    }
                }
            });
        });
    }

    private boolean areClusterStatesCompatibleForRebalance(ClusterState source, ClusterState target) {
        List<DiscoveryNode> sourceNodes = getAssignableNodes(source);
        List<DiscoveryNode> targetNodes = getAssignableNodes(target);
        // We also compare node loads as it could be that another ML job has been started meanwhile
        return sourceNodes.equals(targetNodes)
            && detectNodeLoads(sourceNodes, source).equals(detectNodeLoads(targetNodes, target))
            && MlMetadata.getMlMetadata(source).equals(MlMetadata.getMlMetadata(target))
            && TrainedModelAssignmentMetadata.fromState(source).equals(TrainedModelAssignmentMetadata.fromState(target));
    }

    private TrainedModelAssignmentMetadata.Builder rebalanceAssignments(
        ClusterState currentState,
        Optional<StartTrainedModelDeploymentAction.TaskParams> modelToAdd
    ) throws Exception {
        List<DiscoveryNode> nodes = getAssignableNodes(currentState);
        logger.debug(() -> format("assignable nodes are %s", nodes.stream().map(DiscoveryNode::getId).toList()));
        Map<DiscoveryNode, NodeLoad> nodeLoads = detectNodeLoads(nodes, currentState);
        TrainedModelAssignmentRebalancer rebalancer = new TrainedModelAssignmentRebalancer(
            TrainedModelAssignmentMetadata.fromState(currentState),
            nodeLoads,
            nodeAvailabilityZoneMapper.buildMlNodesByAvailabilityZone(currentState),
            modelToAdd
        );
        TrainedModelAssignmentMetadata.Builder rebalanced = rebalancer.rebalance();
        if (modelToAdd.isPresent()) {
            checkModelIsFullyAllocatedIfScalingIsNotPossible(modelToAdd.get().getModelId(), rebalanced, nodes);
        }
        return rebalanced;
    }

    private void checkModelIsFullyAllocatedIfScalingIsNotPossible(
        String modelId,
        TrainedModelAssignmentMetadata.Builder assignments,
        List<DiscoveryNode> nodes
    ) {
        TrainedModelAssignment assignment = assignments.getAssignment(modelId).build();
        if (isScalingPossible(nodes) || assignment.isSatisfied(nodes.stream().map(DiscoveryNode::getId).collect(Collectors.toSet()))) {
            return;
        }

        if (assignment.getNodeRoutingTable().isEmpty()) {
            String msg = "Could not start deployment because no suitable nodes were found, allocation explanation ["
                + assignment.getReason().orElse("none")
                + "]";
            logger.warn("[{}] {}", modelId, msg);
            Exception detail = new IllegalStateException(msg);
            throw new ElasticsearchStatusException(
                "Could not start deployment because no ML nodes with sufficient capacity were found",
                RestStatus.TOO_MANY_REQUESTS,
                detail
            );
        }

        String msg = "Could not start deployment because there are not enough resources to provide all requested allocations";
        logger.debug(() -> format("[%s] %s", modelId, msg));
        throw new ElasticsearchStatusException(msg, RestStatus.TOO_MANY_REQUESTS);
    }

    private static List<DiscoveryNode> getAssignableNodes(ClusterState clusterState) {
        final Set<String> shuttingDownNodes = nodesShuttingDown(clusterState);
        return clusterState.getNodes()
            .getNodes()
            .values()
            .stream()
            .filter(StartTrainedModelDeploymentAction.TaskParams::mayAssignToNode)
            .filter(n -> shuttingDownNodes.contains(n.getId()) == false)
            .toList();
    }

    private Map<DiscoveryNode, NodeLoad> detectNodeLoads(List<DiscoveryNode> nodes, ClusterState clusterState) {
        return nodes.stream()
            .collect(
                Collectors.toMap(
                    Function.identity(),
                    n -> nodeLoadDetector.detectNodeLoad(clusterState, null, n, maxOpenJobs, maxMemoryPercentage, useAuto)
                )
            );
    }

    private boolean isScalingPossible(List<DiscoveryNode> nodes) {
        OptionalLong smallestMLNode = nodes.stream().map(NodeLoadDetector::getNodeSize).flatMapToLong(OptionalLong::stream).min();

        // We can scale horizontally
        return maxLazyMLNodes > nodes.size()
            // We can scale vertically

            // TODO This checks if there is more space we could vertically scale to but
            // not if it will be enough for the model to actually fit in. For example,
            // we might be 32GB off of the maximum ML tier size and someone wants to start a 45GB model.
            // As this code stands we'll scale up to maximum size then find we still cannot start that model.
            || (smallestMLNode.isPresent() && smallestMLNode.getAsLong() < maxMLNodeSize);
    }

    public void updateNumberOfAllocations(String modelId, int numberOfAllocations, ActionListener<TrainedModelAssignment> listener) {
        updateNumberOfAllocations(clusterService.state(), modelId, numberOfAllocations, listener);
    }

    private void updateNumberOfAllocations(
        ClusterState clusterState,
        String modelId,
        int numberOfAllocations,
        ActionListener<TrainedModelAssignment> listener
    ) {
        TrainedModelAssignmentMetadata metadata = TrainedModelAssignmentMetadata.fromState(clusterState);
        final TrainedModelAssignment existingAssignment = metadata.getModelAssignment(modelId);
        if (existingAssignment == null) {
            throw new ResourceNotFoundException("deployment for model with id [{}] not found", modelId);
        }
        if (existingAssignment.getTaskParams().getNumberOfAllocations() == numberOfAllocations) {
            listener.onResponse(existingAssignment);
            return;
        }
        if (existingAssignment.getAssignmentState() != AssignmentState.STARTED) {
            listener.onFailure(
                new ElasticsearchStatusException(
                    "cannot update deployment that is not in [{}] state",
                    RestStatus.CONFLICT,
                    AssignmentState.STARTED
                )
            );
            return;
        }
        if (clusterState.nodes().getMinNodeVersion().before(DISTRIBUTED_MODEL_ALLOCATION_VERSION)) {
            listener.onFailure(
                new ElasticsearchStatusException(
                    "cannot update number_of_allocations for deployment with model id [{}] while there are nodes older than version [{}]",
                    RestStatus.CONFLICT,
                    modelId,
                    DISTRIBUTED_MODEL_ALLOCATION_VERSION
                )
            );
            return;
        }

        ActionListener<ClusterState> updatedStateListener = ActionListener.wrap(
            updatedState -> submitUnbatchedTask("update model deployment number_of_allocations", new ClusterStateUpdateTask() {

                private volatile boolean isUpdated;

                @Override
                public ClusterState execute(ClusterState currentState) {
                    if (areClusterStatesCompatibleForRebalance(clusterState, currentState)) {
                        isUpdated = true;
                        return updatedState;
                    }
                    logger.debug(() -> format("[%s] Retrying update as cluster state has been modified", modelId));
                    updateNumberOfAllocations(currentState, modelId, numberOfAllocations, listener);
                    return currentState;
                }

                @Override
                public void onFailure(Exception e) {
                    listener.onFailure(e);
                }

                @Override
                public void clusterStateProcessed(ClusterState oldState, ClusterState newState) {
                    if (isUpdated) {
                        TrainedModelAssignment updatedAssignment = TrainedModelAssignmentMetadata.fromState(newState)
                            .getModelAssignment(modelId);
                        if (updatedAssignment.totalTargetAllocations() > existingAssignment.totalTargetAllocations()) {
                            threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME)
                                .execute(
                                    () -> systemAuditor.info(
                                        Messages.getMessage(Messages.INFERENCE_DEPLOYMENT_REBALANCED, "model deployment updated")
                                    )
                                );
                        }
                        listener.onResponse(updatedAssignment);
                    }
                }
            }),
            listener::onFailure
        );

        adjustNumberOfAllocations(clusterState, existingAssignment, numberOfAllocations, updatedStateListener);
    }

    private void adjustNumberOfAllocations(
        ClusterState clusterState,
        TrainedModelAssignment assignment,
        int numberOfAllocations,
        ActionListener<ClusterState> listener
    ) {
        threadPool.executor(MachineLearning.UTILITY_THREAD_POOL_NAME).execute(() -> {
            if (numberOfAllocations > assignment.getTaskParams().getNumberOfAllocations()) {
                increaseNumberOfAllocations(clusterState, assignment, numberOfAllocations, listener);
            } else {
                decreaseNumberOfAllocations(clusterState, assignment, numberOfAllocations, listener);
            }
        });
    }

    private void increaseNumberOfAllocations(
        ClusterState clusterState,
        TrainedModelAssignment assignment,
        int numberOfAllocations,
        ActionListener<ClusterState> listener
    ) {
        try {
            final ClusterState updatedClusterState = update(
                clusterState,
                TrainedModelAssignmentMetadata.builder(clusterState)
                    .updateAssignment(
                        assignment.getModelId(),
                        TrainedModelAssignment.Builder.fromAssignment(assignment).setNumberOfAllocations(numberOfAllocations)
                    )
            );
            TrainedModelAssignmentMetadata.Builder rebalancedMetadata = rebalanceAssignments(updatedClusterState, Optional.empty());
            if (isScalingPossible(getAssignableNodes(clusterState)) == false
                && rebalancedMetadata.getAssignment(assignment.getModelId()).build().totalTargetAllocations() < numberOfAllocations) {
                listener.onFailure(
                    new ElasticsearchStatusException(
                        "Could not update deployment because there are not enough resources to provide all requested allocations",
                        RestStatus.TOO_MANY_REQUESTS
                    )
                );
            } else {
                listener.onResponse(update(clusterState, rebalancedMetadata));
            }
        } catch (Exception e) {
            listener.onFailure(e);
        }
    }

    private void decreaseNumberOfAllocations(
        ClusterState clusterState,
        TrainedModelAssignment assignment,
        int numberOfAllocations,
        ActionListener<ClusterState> listener
    ) {
        TrainedModelAssignment.Builder updatedAssignment = numberOfAllocations < assignment.totalTargetAllocations()
            ? new AllocationReducer(assignment, nodeAvailabilityZoneMapper.buildMlNodesByAvailabilityZone(clusterState)).reduceTo(
                numberOfAllocations
            )
            : TrainedModelAssignment.Builder.fromAssignment(assignment).setNumberOfAllocations(numberOfAllocations);

        // We have now reduced allocations to a number we can be sure it is satisfied
        // and thus we should clear the assignment reason.
        if (numberOfAllocations <= assignment.totalTargetAllocations()) {
            updatedAssignment.setReason(null);
        }
        TrainedModelAssignmentMetadata.Builder builder = TrainedModelAssignmentMetadata.builder(clusterState);
        builder.updateAssignment(assignment.getModelId(), updatedAssignment);
        listener.onResponse(update(clusterState, builder));
    }

    static ClusterState setToStopping(ClusterState clusterState, String modelId, String reason) {
        TrainedModelAssignmentMetadata metadata = TrainedModelAssignmentMetadata.fromState(clusterState);
        final TrainedModelAssignment existingAssignment = metadata.getModelAssignment(modelId);
        if (existingAssignment == null) {
            throw new ResourceNotFoundException("assignment for model with id [{}] not found", modelId);
        }
        // If we are stopping, don't update anything
        if (existingAssignment.getAssignmentState().equals(AssignmentState.STOPPING)) {
            return clusterState;
        }
        TrainedModelAssignmentMetadata.Builder builder = TrainedModelAssignmentMetadata.builder(clusterState);
        builder.getAssignment(modelId).stopAssignment(reason);
        return update(clusterState, builder);
    }

    static ClusterState updateModelRoutingTable(ClusterState currentState, UpdateTrainedModelAssignmentRoutingInfoAction.Request request) {
        final String modelId = request.getModelId();
        final String nodeId = request.getNodeId();
        TrainedModelAssignmentMetadata metadata = TrainedModelAssignmentMetadata.fromState(currentState);
        logger.trace(() -> format("[%s] [%s] current metadata before update %s", modelId, nodeId, Strings.toString(metadata)));
        final TrainedModelAssignment existingAssignment = metadata.getModelAssignment(modelId);
        final TrainedModelAssignmentMetadata.Builder builder = TrainedModelAssignmentMetadata.builder(currentState);
        // If state is stopped, this indicates the node process is closed, remove the node from the assignment
        if (request.getUpdate().getStateAndReason().isPresent()
            && request.getUpdate().getStateAndReason().get().getState().equals(RoutingState.STOPPED)) {
            if (existingAssignment == null || existingAssignment.isRoutedToNode(nodeId) == false) {
                return currentState;
            }
            builder.getAssignment(modelId).removeRoutingEntry(nodeId).calculateAndSetAssignmentState();
            return update(currentState, builder);
        }

        if (existingAssignment == null) {
            throw new ResourceNotFoundException("assignment for model with id [{}] not found", modelId);
        }
        // If we are stopping, don't update anything
        if (existingAssignment.getAssignmentState().equals(AssignmentState.STOPPING)) {
            logger.debug(
                () -> format("[%s] requested update from node [%s] while stopping; update was [%s]", modelId, nodeId, request.getUpdate())
            );
            return currentState;
        }
        if (existingAssignment.isRoutedToNode(nodeId) == false) {
            throw new ResourceNotFoundException("assignment for model with id [{}]] is not routed to node [{}]", modelId, nodeId);
        }
        RoutingInfo routingInfo = existingAssignment.getNodeRoutingTable().get(nodeId);
        builder.getAssignment(modelId)
            .updateExistingRoutingEntry(nodeId, request.getUpdate().apply(routingInfo))
            .calculateAndSetAssignmentState();

        return update(currentState, builder);
    }

    static ClusterState removeAssignment(ClusterState currentState, String modelId) {
        TrainedModelAssignmentMetadata.Builder builder = TrainedModelAssignmentMetadata.builder(currentState);
        if (builder.hasModel(modelId) == false) {
            throw new ResourceNotFoundException("assignment for model with id [{}] not found", modelId);
        }
        logger.debug(() -> format("[%s] removing assignment", modelId));
        return update(currentState, builder.removeAssignment(modelId));
    }

    static ClusterState removeAllAssignments(ClusterState currentState) {
        if (TrainedModelAssignmentMetadata.fromState(currentState).modelAssignments().isEmpty()) {
            return currentState;
        }
        return forceUpdate(currentState, TrainedModelAssignmentMetadata.Builder.empty());
    }

    static Optional<String> detectReasonToRebalanceModels(final ClusterChangedEvent event) {
        // If there are no assignments created at all, there is nothing to update
        final TrainedModelAssignmentMetadata newMetadata = TrainedModelAssignmentMetadata.fromState(event.state());
        if (newMetadata == null || newMetadata.modelAssignments().isEmpty()) {
            return Optional.empty();
        }

        // If an ML persistent task with process stopped we should rebalance as we could have
        // available memory that we did not have before.
        return detectReasonIfMlJobsStopped(event).or(() -> {
            String reason = null;
            if (haveMlNodesChanged(event, newMetadata)) {
                reason = "nodes changed";
            } else if (newMetadata.hasOutdatedAssignments()) {
                reason = "outdated assignments detected";
            }
            return Optional.ofNullable(reason);
        });
    }

    static Optional<String> detectReasonIfMlJobsStopped(ClusterChangedEvent event) {
        if (event.changedCustomMetadataSet().contains(PersistentTasksCustomMetadata.TYPE) == false) {
            return Optional.empty();
        }
        final PersistentTasksCustomMetadata previousPersistentTasks = event.previousState()
            .getMetadata()
            .custom(PersistentTasksCustomMetadata.TYPE);
        final PersistentTasksCustomMetadata currentPersistentTasks = event.state().getMetadata().custom(PersistentTasksCustomMetadata.TYPE);
        Set<String> previousMlTaskIds = findMlProcessTaskIds(previousPersistentTasks);
        Set<String> currentMlTaskIds = findMlProcessTaskIds(currentPersistentTasks);
        Set<String> stoppedTaskTypes = previousMlTaskIds.stream()
            .filter(id -> currentMlTaskIds.contains(id) == false) // remove the tasks that are still present. Stopped Ids only.
            .map(previousPersistentTasks::getTask)
            .map(PersistentTasksCustomMetadata.PersistentTask::getTaskName)
            .map(MlTasks::prettyPrintTaskName)
            .collect(Collectors.toSet());
        if (stoppedTaskTypes.size() == 1) {
            return Optional.of("ML [" + stoppedTaskTypes.iterator().next() + "] job stopped");
        } else if (stoppedTaskTypes.size() > 1) {
            return Optional.of("ML " + stoppedTaskTypes + " jobs stopped");
        }
        return Optional.empty();
    }

    private static Set<String> findMlProcessTaskIds(@Nullable PersistentTasksCustomMetadata metadata) {
        return metadata == null
            ? Set.of()
            : MlTasks.findMlProcessTasks(metadata)
                .stream()
                .map(PersistentTasksCustomMetadata.PersistentTask::getId)
                .collect(Collectors.toSet());
    }

    static boolean haveMlNodesChanged(ClusterChangedEvent event, TrainedModelAssignmentMetadata newMetadata) {
        // Reallocate in reaction to either node change events or
        // changes triggered by the node shutdown API.
        // When the shutdown API is used the metadata is modified
        // before the node is removed and then once again after
        // the node has returned. In this situation the node change
        // events become a no-op due to the checks against shutting
        // down nodes and because reassignment has already been
        // triggered by the node shutdown metadata changes.
        //
        // If the shutdown API is not used the node change events
        // are sufficient to cause a reassignment.
        //
        // Shutdowns should be respected so that the service does not
        // allocate models to a node that is about to leave the cluster
        //
        // TODO this has a weird side-effect for allocating to nodes
        // If the event indicates there were nodes added/removed, this method only looks at the current state and has
        // no previous knowledge of existing nodes. Consequently, if a model was manually removed (task-kill) from a node
        // it may get re-allocated to that node when another node is added/removed...
        boolean nodesShutdownChanged = event.changedCustomMetadataSet().contains(NodesShutdownMetadata.TYPE);
        if (event.nodesChanged() || nodesShutdownChanged) {
            Set<String> shuttingDownNodes = nodesShuttingDown(event.state());
            DiscoveryNodes.Delta nodesDelta = event.nodesDelta();

            Set<String> removedNodes = nodesDelta.removedNodes().stream().map(DiscoveryNode::getId).collect(Collectors.toSet());
            Set<String> addedNodes = nodesDelta.addedNodes().stream().map(DiscoveryNode::getId).collect(Collectors.toSet());

            Set<String> exitingShutDownNodes;
            if (nodesShutdownChanged) {
                Set<String> previousShuttingDownNodes = nodesShuttingDown(event.previousState());

                // Add nodes that where marked for shutdown in the previous state
                // but are no longer marked as shutdown in the current state.
                Set<String> returningShutDownNodes = Sets.difference(previousShuttingDownNodes, shuttingDownNodes);
                addedNodes.addAll(returningShutDownNodes);

                // and nodes that are marked for shutdown in this event only
                exitingShutDownNodes = Sets.difference(shuttingDownNodes, previousShuttingDownNodes);
                removedNodes.addAll(exitingShutDownNodes);
            } else {
                exitingShutDownNodes = Collections.emptySet();
            }

            logger.debug(
                () -> format(
                    "added nodes %s; removed nodes %s; shutting down nodes %s; exiting shutdown nodes %s",
                    addedNodes,
                    removedNodes,
                    shuttingDownNodes,
                    exitingShutDownNodes
                )
            );
            for (TrainedModelAssignment trainedModelAssignment : newMetadata.modelAssignments().values()) {
                if (trainedModelAssignment.getAssignmentState().equals(AssignmentState.STOPPING)) {
                    continue;
                }
                for (var nodeId : exitingShutDownNodes) {
                    if (trainedModelAssignment.isRoutedToNode(nodeId)) {
                        logger.debug(
                            () -> format(
                                "should rebalance because model [%s] has allocations on shutting down node [%s]",
                                trainedModelAssignment.getModelId(),
                                nodeId
                            )
                        );
                        return true;
                    }
                }

                for (var nodeId : removedNodes) {
                    if (trainedModelAssignment.isRoutedToNode(nodeId) && shuttingDownNodes.contains(nodeId) == false) {
                        logger.debug(
                            () -> format(
                                "should rebalance because model [%s] has allocations on removed node [%s]",
                                trainedModelAssignment.getModelId(),
                                nodeId
                            )
                        );
                        return true;
                    }
                }
                for (var nodeId : addedNodes) {
                    if (StartTrainedModelDeploymentAction.TaskParams.mayAssignToNode(event.state().nodes().get(nodeId))
                        && shuttingDownNodes.contains(nodeId) == false) {
                        logger.debug(() -> format("should rebalance because ML eligible node [%s] was added", nodeId));
                        return true;
                    }
                }
            }
        }
        return false;
    }

    /**
     * Returns the set of nodes that are currently shutting down
     */
    static Set<String> nodesShuttingDown(final ClusterState state) {
        return NodesShutdownMetadata.getShutdowns(state)
            .map(NodesShutdownMetadata::getAllNodeMetadataMap)
            .map(Map::keySet)
            .orElse(Collections.emptySet());
    }
}
