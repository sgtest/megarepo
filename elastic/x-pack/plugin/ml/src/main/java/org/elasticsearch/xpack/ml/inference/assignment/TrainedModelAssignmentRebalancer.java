/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.assignment;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.logging.log4j.message.ParameterizedMessage;
import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.inference.assignment.Priority;
import org.elasticsearch.xpack.core.ml.inference.assignment.RoutingInfo;
import org.elasticsearch.xpack.core.ml.inference.assignment.RoutingState;
import org.elasticsearch.xpack.core.ml.inference.assignment.TrainedModelAssignment;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.inference.assignment.planning.AssignmentPlan;
import org.elasticsearch.xpack.ml.inference.assignment.planning.AssignmentPlanner;
import org.elasticsearch.xpack.ml.inference.assignment.planning.ZoneAwareAssignmentPlanner;
import org.elasticsearch.xpack.ml.job.NodeLoad;
import org.elasticsearch.xpack.ml.utils.MlProcessors;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.TreeMap;
import java.util.function.Function;
import java.util.stream.Collectors;

import static org.elasticsearch.core.Strings.format;
import static org.elasticsearch.xpack.ml.MachineLearning.MAX_LOW_PRIORITY_MODELS_PER_NODE;

class TrainedModelAssignmentRebalancer {

    private static final Logger logger = LogManager.getLogger(TrainedModelAssignmentRebalancer.class);

    private final TrainedModelAssignmentMetadata currentMetadata;
    private final Map<DiscoveryNode, NodeLoad> nodeLoads;
    private final Map<List<String>, Collection<DiscoveryNode>> mlNodesByZone;
    private final Optional<StartTrainedModelDeploymentAction.TaskParams> modelToAdd;

    TrainedModelAssignmentRebalancer(
        TrainedModelAssignmentMetadata currentMetadata,
        Map<DiscoveryNode, NodeLoad> nodeLoads,
        Map<List<String>, Collection<DiscoveryNode>> mlNodesByZone,
        Optional<StartTrainedModelDeploymentAction.TaskParams> modelToAdd
    ) {
        this.currentMetadata = Objects.requireNonNull(currentMetadata);
        this.nodeLoads = Objects.requireNonNull(nodeLoads);
        this.mlNodesByZone = Objects.requireNonNull(mlNodesByZone);
        this.modelToAdd = Objects.requireNonNull(modelToAdd);
    }

    TrainedModelAssignmentMetadata.Builder rebalance() throws Exception {
        if (modelToAdd.isPresent() && currentMetadata.hasModel(modelToAdd.get().getModelId())) {
            throw new ResourceAlreadyExistsException("assignment for model with id [{}] already exists", modelToAdd.get().getModelId());
        }

        if (modelToAdd.isEmpty() && areAllModelsSatisfiedAndNoOutdatedRoutingEntries()) {
            logger.trace(() -> "No need to rebalance as all model deployments are satisfied");
            return TrainedModelAssignmentMetadata.Builder.fromMetadata(currentMetadata);
        }

        AssignmentPlan assignmentPlan = computeAssignmentPlan();
        return buildAssignmentsFromPlan(assignmentPlan);
    }

    private boolean areAllModelsSatisfiedAndNoOutdatedRoutingEntries() {
        Set<String> assignableNodeIds = nodeLoads.keySet().stream().map(DiscoveryNode::getId).collect(Collectors.toSet());
        for (TrainedModelAssignment model : currentMetadata.modelAssignments().values()) {
            if (model.isSatisfied(assignableNodeIds) == false || model.hasOutdatedRoutingEntries()) {
                return false;
            }
        }
        return true;
    }

    AssignmentPlan computeAssignmentPlan() {
        final Map<List<String>, List<AssignmentPlan.Node>> nodesByZone = createNodesByZoneMap();
        final Set<String> assignableNodeIds = nodesByZone.values()
            .stream()
            .flatMap(List::stream)
            .map(AssignmentPlan.Node::id)
            .collect(Collectors.toSet());

        AssignmentPlan planForNormalPriorityModels = computePlanForNormalPriorityModels(nodesByZone, assignableNodeIds);
        AssignmentPlan planForLowPriorityModels = computePlanForLowPriorityModels(assignableNodeIds, planForNormalPriorityModels);
        return mergePlans(nodesByZone, planForNormalPriorityModels, planForLowPriorityModels);
    }

    private AssignmentPlan mergePlans(
        Map<List<String>, List<AssignmentPlan.Node>> nodesByZone,
        AssignmentPlan planForNormalPriorityModels,
        AssignmentPlan planForLowPriorityModels
    ) {
        final List<AssignmentPlan.Node> allNodes = new ArrayList<>();
        nodesByZone.values().forEach(allNodes::addAll);

        final List<AssignmentPlan.Model> allModels = new ArrayList<>();
        allModels.addAll(planForNormalPriorityModels.models());
        allModels.addAll(planForLowPriorityModels.models());

        final Map<String, AssignmentPlan.Node> originalNodeById = allNodes.stream()
            .collect(Collectors.toMap(AssignmentPlan.Node::id, Function.identity()));
        AssignmentPlan.Builder finalPlanBuilder = AssignmentPlan.builder(allNodes, allModels);
        copyAssignments(planForNormalPriorityModels, finalPlanBuilder, originalNodeById);
        copyAssignments(planForLowPriorityModels, finalPlanBuilder, originalNodeById);
        return finalPlanBuilder.build();
    }

    private static void copyAssignments(
        AssignmentPlan source,
        AssignmentPlan.Builder dest,
        Map<String, AssignmentPlan.Node> originalNodeById
    ) {
        for (AssignmentPlan.Model m : source.models()) {
            Map<AssignmentPlan.Node, Integer> nodeAssignments = source.assignments(m).orElse(Map.of());
            for (Map.Entry<AssignmentPlan.Node, Integer> assignment : nodeAssignments.entrySet()) {
                AssignmentPlan.Node originalNode = originalNodeById.get(assignment.getKey().id());
                dest.assignModelToNode(m, originalNode, assignment.getValue());
                if (m.currentAllocationsByNodeId().containsKey(originalNode.id())) {
                    // As the node has all its available memory we need to manually account memory of models with
                    // current allocations.
                    dest.accountMemory(m, originalNode);
                }
            }
        }
    }

    private AssignmentPlan computePlanForNormalPriorityModels(
        Map<List<String>, List<AssignmentPlan.Node>> nodesByZone,
        Set<String> assignableNodeIds
    ) {
        final List<AssignmentPlan.Model> planModels = new ArrayList<>();

        currentMetadata.modelAssignments()
            .values()
            .stream()
            .filter(assignment -> assignment.getTaskParams().getPriority() != Priority.LOW)
            .map(assignment -> {
                Map<String, Integer> currentAssignments = assignment.getNodeRoutingTable()
                    .entrySet()
                    .stream()
                    // Filter out nodes that are no longer assignable
                    .filter(e -> assignableNodeIds.contains(e.getKey()))
                    // Filter out allocation without current and target allocations as they are from before using the rebalancer
                    .filter(e -> e.getValue().getCurrentAllocations() > 0 && e.getValue().getTargetAllocations() > 0)
                    .filter(e -> e.getValue().getState().isAnyOf(RoutingState.STARTING, RoutingState.STARTED, RoutingState.FAILED))
                    .collect(Collectors.toMap(Map.Entry::getKey, e -> e.getValue().getTargetAllocations()));
                return new AssignmentPlan.Model(
                    assignment.getModelId(),
                    assignment.getTaskParams().estimateMemoryUsageBytes(),
                    assignment.getTaskParams().getNumberOfAllocations(),
                    assignment.getTaskParams().getThreadsPerAllocation(),
                    currentAssignments,
                    assignment.getMaxAssignedAllocations()
                );
            })
            .forEach(planModels::add);
        if (modelToAdd.isPresent() && modelToAdd.get().getPriority() != Priority.LOW) {
            StartTrainedModelDeploymentAction.TaskParams taskParams = modelToAdd.get();
            planModels.add(
                new AssignmentPlan.Model(
                    taskParams.getModelId(),
                    taskParams.estimateMemoryUsageBytes(),
                    taskParams.getNumberOfAllocations(),
                    taskParams.getThreadsPerAllocation(),
                    Map.of(),
                    0
                )
            );
        }
        return new ZoneAwareAssignmentPlanner(nodesByZone, planModels).computePlan();
    }

    private AssignmentPlan computePlanForLowPriorityModels(Set<String> assignableNodeIds, AssignmentPlan planExcludingLowPriorityModels) {
        List<AssignmentPlan.Node> planNodes = mlNodesByZone.values()
            .stream()
            .flatMap(Collection::stream)
            .map(
                discoveryNode -> new AssignmentPlan.Node(
                    discoveryNode.getId(),
                    planExcludingLowPriorityModels.getRemainingNodeMemory(discoveryNode.getId()),
                    MAX_LOW_PRIORITY_MODELS_PER_NODE
                )
            )
            .toList();

        final Map<String, Long> remainingNodeMemory = new HashMap<>();
        planNodes.forEach(n -> remainingNodeMemory.put(n.id(), n.availableMemoryBytes()));

        final List<AssignmentPlan.Model> planModels = new ArrayList<>();
        currentMetadata.modelAssignments()
            .values()
            .stream()
            .filter(assignment -> assignment.getTaskParams().getPriority() == Priority.LOW)
            .sorted(Comparator.comparingLong(assignment -> assignment.getTaskParams().estimateMemoryUsageBytes()))
            .map(
                assignment -> new AssignmentPlan.Model(
                    assignment.getModelId(),
                    assignment.getTaskParams().estimateMemoryUsageBytes(),
                    assignment.getTaskParams().getNumberOfAllocations(),
                    assignment.getTaskParams().getThreadsPerAllocation(),
                    findFittingAssignments(assignment, assignableNodeIds, remainingNodeMemory),
                    assignment.getMaxAssignedAllocations(),
                    Priority.LOW
                )
            )
            .forEach(planModels::add);
        if (modelToAdd.isPresent() && modelToAdd.get().getPriority() == Priority.LOW) {
            StartTrainedModelDeploymentAction.TaskParams taskParams = modelToAdd.get();
            planModels.add(
                new AssignmentPlan.Model(
                    taskParams.getModelId(),
                    taskParams.estimateMemoryUsageBytes(),
                    taskParams.getNumberOfAllocations(),
                    taskParams.getThreadsPerAllocation(),
                    Map.of(),
                    0,
                    Priority.LOW
                )
            );
        }

        logger.debug(
            () -> format("Computing plan for low priority deployments. CPU cores fixed to [%s].", MAX_LOW_PRIORITY_MODELS_PER_NODE)
        );

        // No need to use the zone aware planner as there is only 1 allocation for low priority models.
        return new AssignmentPlanner(planNodes, planModels).computePlan();
    }

    private Map<String, Integer> findFittingAssignments(
        TrainedModelAssignment assignment,
        Set<String> assignableNodeIds,
        Map<String, Long> remainingNodeMemory
    ) {
        Map<String, Integer> currentAssignments = assignment.getNodeRoutingTable()
            .entrySet()
            .stream()
            // Filter out nodes that are no longer assignable
            .filter(e -> assignableNodeIds.contains(e.getKey()))
            .filter(e -> e.getValue().getState().isAnyOf(RoutingState.STARTING, RoutingState.STARTED, RoutingState.FAILED))
            .collect(Collectors.toMap(Map.Entry::getKey, e -> e.getValue().getTargetAllocations()));

        final long modelMemoryBytes = assignment.getTaskParams().estimateMemoryUsageBytes();
        Map<String, Integer> fittingAssignments = new HashMap<>();
        currentAssignments.entrySet().stream().filter(nodeToAllocations -> nodeToAllocations.getValue() > 0).forEach(nodeToAllocations -> {
            if (remainingNodeMemory.get(nodeToAllocations.getKey()) >= modelMemoryBytes) {
                fittingAssignments.put(nodeToAllocations.getKey(), nodeToAllocations.getValue());
                remainingNodeMemory.computeIfPresent(nodeToAllocations.getKey(), (k, v) -> v - modelMemoryBytes);
            }
        });
        return fittingAssignments;
    }

    private Map<List<String>, List<AssignmentPlan.Node>> createNodesByZoneMap() {
        return mlNodesByZone.entrySet().stream().collect(Collectors.toMap(e -> e.getKey(), e -> {
            Collection<DiscoveryNode> discoveryNodes = e.getValue();
            List<AssignmentPlan.Node> nodes = new ArrayList<>();
            for (DiscoveryNode discoveryNode : discoveryNodes) {
                if (nodeLoads.containsKey(discoveryNode)) {
                    NodeLoad load = nodeLoads.get(discoveryNode);
                    if (Strings.isNullOrEmpty(load.getError())) {
                        nodes.add(
                            new AssignmentPlan.Node(
                                discoveryNode.getId(),
                                // We subtract native inference memory as the planner expects available memory for
                                // native inference including current assignments.
                                getNodeFreeMemoryExcludingPerNodeOverheadAndNativeInference(load),
                                MlProcessors.get(discoveryNode).roundUp()
                            )
                        );
                    } else {
                        logger.warn(
                            format("ignoring node [%s] as detecting its load failed with [%s]", discoveryNode.getId(), load.getError())
                        );
                    }
                } else {
                    logger.warn(format("ignoring node [%s] as no load could be detected", discoveryNode.getId()));
                }
            }
            return nodes;
        }));
    }

    private static long getNodeFreeMemoryExcludingPerNodeOverheadAndNativeInference(NodeLoad load) {
        return load.getFreeMemoryExcludingPerNodeOverhead() - load.getAssignedNativeInferenceMemory();
    }

    private TrainedModelAssignmentMetadata.Builder buildAssignmentsFromPlan(AssignmentPlan assignmentPlan) {
        TrainedModelAssignmentMetadata.Builder builder = TrainedModelAssignmentMetadata.Builder.empty();
        for (AssignmentPlan.Model model : assignmentPlan.models()) {
            TrainedModelAssignment existingAssignment = currentMetadata.getModelAssignment(model.id());

            TrainedModelAssignment.Builder assignmentBuilder = TrainedModelAssignment.Builder.empty(
                existingAssignment == null && modelToAdd.isPresent()
                    ? modelToAdd.get()
                    : currentMetadata.getModelAssignment(model.id()).getTaskParams()
            );
            if (existingAssignment != null) {
                assignmentBuilder.setStartTime(existingAssignment.getStartTime());
                assignmentBuilder.setMaxAssignedAllocations(existingAssignment.getMaxAssignedAllocations());
            }

            Map<AssignmentPlan.Node, Integer> assignments = assignmentPlan.assignments(model).orElseGet(Map::of);
            for (Map.Entry<AssignmentPlan.Node, Integer> assignment : assignments.entrySet()) {
                if (existingAssignment != null && existingAssignment.isRoutedToNode(assignment.getKey().id())) {
                    RoutingInfo existingRoutingInfo = existingAssignment.getNodeRoutingTable().get(assignment.getKey().id());
                    RoutingState state = existingRoutingInfo.getState();
                    String reason = existingRoutingInfo.getReason();
                    if (state == RoutingState.FAILED) {
                        state = RoutingState.STARTING;
                        reason = "";
                    }
                    assignmentBuilder.addRoutingEntry(
                        assignment.getKey().id(),
                        new RoutingInfo(existingRoutingInfo.getCurrentAllocations(), assignment.getValue(), state, reason)
                    );
                } else {
                    assignmentBuilder.addRoutingEntry(
                        assignment.getKey().id(),
                        new RoutingInfo(assignment.getValue(), assignment.getValue(), RoutingState.STARTING, "")
                    );
                }
            }
            assignmentBuilder.calculateAndSetAssignmentState();

            explainAssignments(assignmentPlan, nodeLoads, model).ifPresent(assignmentBuilder::setReason);
            builder.addNewAssignment(model.id(), assignmentBuilder);
        }
        return builder;
    }

    private Optional<String> explainAssignments(
        AssignmentPlan assignmentPlan,
        Map<DiscoveryNode, NodeLoad> nodeLoads,
        AssignmentPlan.Model model
    ) {
        if (assignmentPlan.satisfiesAllocations(model)) {
            return Optional.empty();
        }

        if (nodeLoads.isEmpty()) {
            return Optional.of("No ML nodes exist in the cluster");
        }

        Map<String, String> nodeToReason = new TreeMap<>();
        for (Map.Entry<DiscoveryNode, NodeLoad> nodeAndLoad : nodeLoads.entrySet()) {
            Optional<String> reason = explainAssignment(assignmentPlan, nodeAndLoad.getKey(), nodeAndLoad.getValue(), model);
            reason.ifPresent(s -> nodeToReason.put(nodeAndLoad.getKey().getId(), s));
        }

        if (nodeToReason.isEmpty() == false) {
            return Optional.of(
                nodeToReason.entrySet()
                    .stream()
                    .map(entry -> format("Could not assign (more) allocations on node [%s]. Reason: %s", entry.getKey(), entry.getValue()))
                    .collect(Collectors.joining("|"))
            );
        }
        return Optional.empty();
    }

    private Optional<String> explainAssignment(
        AssignmentPlan assignmentPlan,
        DiscoveryNode node,
        NodeLoad load,
        AssignmentPlan.Model model
    ) {
        if (Strings.isNullOrEmpty(load.getError()) == false) {
            return Optional.of(load.getError());
        }

        if (model.memoryBytes() > assignmentPlan.getRemainingNodeMemory(node.getId())) {
            // If any ML processes are running on a node we require some space to load the shared libraries.
            // So if none are currently running then this per-node overhead must be added to the requirement.
            // From node load we know if we had any jobs or models assigned before the rebalance.
            // But we should also check if we managed to assign a model during the rebalance for which
            // we check if the node has used up any of its allocated processors.
            boolean isPerNodeOverheadAccountedFor = load.getNumAssignedJobsAndModels() > 0
                || assignmentPlan.getRemainingNodeCores(load.getNodeId()) < MlProcessors.get(node).roundUp();
            long requiredMemory = model.memoryBytes() + (isPerNodeOverheadAccountedFor
                ? 0
                : MachineLearning.NATIVE_EXECUTABLE_CODE_OVERHEAD.getBytes());
            long nodeFreeMemory = assignmentPlan.getRemainingNodeMemory(node.getId()) + (isPerNodeOverheadAccountedFor
                ? 0
                : MachineLearning.NATIVE_EXECUTABLE_CODE_OVERHEAD.getBytes());
            return Optional.of(
                ParameterizedMessage.format(
                    "This node has insufficient available memory. Available memory for ML [{} ({})], "
                        + "free memory [{} ({})], "
                        + "estimated memory required for this model [{} ({})].",
                    new Object[] {
                        load.getMaxMlMemory(),
                        ByteSizeValue.ofBytes(load.getMaxMlMemory()).toString(),
                        nodeFreeMemory,
                        ByteSizeValue.ofBytes(nodeFreeMemory).toString(),
                        requiredMemory,
                        ByteSizeValue.ofBytes(requiredMemory).toString() }
                )
            );
        }

        if (model.threadsPerAllocation() > assignmentPlan.getRemainingNodeCores(node.getId())) {
            return Optional.of(
                ParameterizedMessage.format(
                    "This node has insufficient allocated processors. Available processors [{}], free processors [{}], "
                        + "processors required for each allocation of this model [{}]",
                    new Object[] {
                        MlProcessors.get(node).roundUp(),
                        assignmentPlan.getRemainingNodeCores(node.getId()),
                        model.threadsPerAllocation() }
                )
            );
        }

        return Optional.empty();
    }
}
