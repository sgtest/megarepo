/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.assignment;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.ResourceAlreadyExistsException;
import org.elasticsearch.ResourceNotFoundException;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.LatchedActionListener;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.metadata.NodesShutdownMetadata;
import org.elasticsearch.cluster.metadata.SingleNodeShutdownMetadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodeRole;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.collect.MapBuilder;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.ml.action.StartTrainedModelDeploymentAction;
import org.elasticsearch.xpack.core.ml.action.UpdateTrainedModelAssignmentRoutingInfoAction;
import org.elasticsearch.xpack.core.ml.inference.assignment.AssignmentState;
import org.elasticsearch.xpack.core.ml.inference.assignment.RoutingInfo;
import org.elasticsearch.xpack.core.ml.inference.assignment.RoutingInfoUpdate;
import org.elasticsearch.xpack.core.ml.inference.assignment.RoutingState;
import org.elasticsearch.xpack.core.ml.inference.assignment.RoutingStateAndReason;
import org.elasticsearch.xpack.core.ml.inference.assignment.TrainedModelAssignment;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.job.NodeLoadDetector;
import org.elasticsearch.xpack.ml.process.MlMemoryTracker;
import org.junit.Before;

import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CountDownLatch;
import java.util.function.Function;

import static org.hamcrest.Matchers.aMapWithSize;
import static org.hamcrest.Matchers.anEmptyMap;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.not;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class TrainedModelAssignmentClusterServiceTests extends ESTestCase {

    private ClusterService clusterService;
    private ThreadPool threadPool;
    private NodeLoadDetector nodeLoadDetector;

    @Before
    public void setupObjects() {
        clusterService = mock(ClusterService.class);
        ClusterSettings clusterSettings = new ClusterSettings(
            Settings.EMPTY,
            Sets.newHashSet(
                MachineLearning.MAX_MACHINE_MEMORY_PERCENT,
                MachineLearning.USE_AUTO_MACHINE_MEMORY_PERCENT,
                MachineLearning.MAX_OPEN_JOBS_PER_NODE
            )
        );
        when(clusterService.getClusterSettings()).thenReturn(clusterSettings);

        threadPool = mock(ThreadPool.class);

        MlMemoryTracker memoryTracker = mock(MlMemoryTracker.class);
        when(memoryTracker.isRecentlyRefreshed()).thenReturn(true);
        nodeLoadDetector = new NodeLoadDetector(memoryTracker);
    }

    public void testUpdateModelRoutingTable() {
        String modelId = "existing-model";
        String nodeId = "ml-node-with-room";
        String startedNode = "started-ml-node-with-room";
        ClusterState currentState = ClusterState.builder(new ClusterName("testUpdateModelRoutingTable"))
            .nodes(
                DiscoveryNodes.builder()
                    .add(buildNode(nodeId, true, ByteSizeValue.ofGb(4).getBytes(), 8))
                    .add(buildNode(startedNode, true, ByteSizeValue.ofGb(4).getBytes(), 8))
                    .build()
            )
            .metadata(
                Metadata.builder()
                    .putCustom(
                        TrainedModelAssignmentMetadata.NAME,
                        TrainedModelAssignmentMetadata.Builder.empty()
                            .addNewAssignment(
                                modelId,
                                TrainedModelAssignment.Builder.empty(newParams(modelId, 10_000L))
                                    .addRoutingEntry(nodeId, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                    .addRoutingEntry(startedNode, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                            )
                            .build()
                    )
                    .build()
            )
            .build();

        assertThatStoppingAssignmentPreventsMutation(
            state -> TrainedModelAssignmentClusterService.updateModelRoutingTable(
                state,
                new UpdateTrainedModelAssignmentRoutingInfoAction.Request(nodeId, modelId, started())
            ),
            currentState
        );

        assertThat(
            TrainedModelAssignmentMetadata.fromState(currentState).getModelAssignment(modelId).getAssignmentState(),
            equalTo(AssignmentState.STARTING)
        );

        ClusterState newState = TrainedModelAssignmentClusterService.updateModelRoutingTable(
            currentState,
            new UpdateTrainedModelAssignmentRoutingInfoAction.Request(startedNode, modelId, started())
        );
        assertThat(
            TrainedModelAssignmentMetadata.fromState(newState)
                .getModelAssignment(modelId)
                .getNodeRoutingTable()
                .get(startedNode)
                .getState(),
            equalTo(RoutingState.STARTED)
        );
        assertThat(
            TrainedModelAssignmentMetadata.fromState(newState).getModelAssignment(modelId).getAssignmentState(),
            equalTo(AssignmentState.STARTED)
        );

        expectThrows(
            ResourceNotFoundException.class,
            () -> TrainedModelAssignmentClusterService.updateModelRoutingTable(
                newState,
                new UpdateTrainedModelAssignmentRoutingInfoAction.Request("missingNode", modelId, started())
            )
        );
        expectThrows(
            ResourceNotFoundException.class,
            () -> TrainedModelAssignmentClusterService.updateModelRoutingTable(
                newState,
                new UpdateTrainedModelAssignmentRoutingInfoAction.Request(nodeId, "missingModel", started())
            )
        );

        // TEST Stopped

        // We should allow a "stopped" update on missing models and nodes as entries may have already been deleted
        TrainedModelAssignmentClusterService.updateModelRoutingTable(
            newState,
            new UpdateTrainedModelAssignmentRoutingInfoAction.Request(
                "missingNode",
                modelId,
                RoutingInfoUpdate.updateStateAndReason(new RoutingStateAndReason(RoutingState.STOPPED, ""))
            )
        );
        TrainedModelAssignmentClusterService.updateModelRoutingTable(
            newState,
            new UpdateTrainedModelAssignmentRoutingInfoAction.Request(
                nodeId,
                "missingModel",
                RoutingInfoUpdate.updateStateAndReason(new RoutingStateAndReason(RoutingState.STOPPED, ""))
            )
        );

        ClusterState updateState = TrainedModelAssignmentClusterService.updateModelRoutingTable(
            newState,
            new UpdateTrainedModelAssignmentRoutingInfoAction.Request(
                nodeId,
                modelId,
                RoutingInfoUpdate.updateStateAndReason(new RoutingStateAndReason(RoutingState.STOPPED, ""))
            )
        );
        assertThat(
            TrainedModelAssignmentMetadata.fromState(updateState).getModelAssignment(modelId).getNodeRoutingTable(),
            not(hasKey(nodeId))
        );
        assertThat(
            TrainedModelAssignmentMetadata.fromState(updateState).getModelAssignment(modelId).getAssignmentState(),
            equalTo(AssignmentState.STARTED)
        );
    }

    public void testRemoveAssignment() {
        ClusterState clusterStateWithoutAssignment = ClusterState.builder(new ClusterName("testRemoveAssignment"))
            .metadata(Metadata.builder().build())
            .build();
        String modelId = "remove-assignment";

        expectThrows(
            ResourceNotFoundException.class,
            () -> TrainedModelAssignmentClusterService.removeAssignment(clusterStateWithoutAssignment, modelId)
        );

        ClusterState clusterStateWithAssignment = ClusterState.builder(new ClusterName("testRemoveAssignment"))
            .nodes(DiscoveryNodes.builder().add(buildNode("test-node", true, ByteSizeValue.ofGb(4).getBytes(), 8)).build())
            .metadata(
                Metadata.builder()
                    .putCustom(
                        TrainedModelAssignmentMetadata.NAME,
                        TrainedModelAssignmentMetadata.Builder.empty()
                            .addNewAssignment(modelId, TrainedModelAssignment.Builder.empty(newParams(modelId, randomNonNegativeLong())))
                            .build()
                    )
                    .build()
            )
            .build();
        assertThat(TrainedModelAssignmentMetadata.fromState(clusterStateWithAssignment).getModelAssignment(modelId), is(not(nullValue())));

        ClusterState modified = TrainedModelAssignmentClusterService.removeAssignment(clusterStateWithAssignment, modelId);
        assertThat(TrainedModelAssignmentMetadata.fromState(modified).getModelAssignment(modelId), is(nullValue()));
    }

    public void testRemoveAllAssignments() {
        ClusterState clusterStateWithoutAssignment = ClusterState.builder(new ClusterName("testRemoveAllAssignments"))
            .metadata(Metadata.builder().build())
            .build();
        assertThat(
            TrainedModelAssignmentClusterService.removeAllAssignments(clusterStateWithoutAssignment),
            equalTo(clusterStateWithoutAssignment)
        );

        ClusterState clusterStateWithAssignments = ClusterState.builder(new ClusterName("testRemoveAllAssignments"))
            .nodes(DiscoveryNodes.builder().add(buildNode("test-node", true, ByteSizeValue.ofGb(4).getBytes(), 8)).build())
            .metadata(
                Metadata.builder()
                    .putCustom(TrainedModelAssignmentMetadata.NAME, TrainedModelAssignmentMetadataTests.randomInstance())
                    .build()
            )
            .build();
        ClusterState modified = TrainedModelAssignmentClusterService.removeAllAssignments(clusterStateWithAssignments);
        assertThat(TrainedModelAssignmentMetadata.fromState(modified).modelAssignments(), is(anEmptyMap()));
    }

    public void testCreateAssignment() throws Exception {
        ClusterState currentState = ClusterState.builder(new ClusterName("testCreateAssignment"))
            .nodes(
                DiscoveryNodes.builder()
                    .add(buildNode("ml-node-with-room", true, ByteSizeValue.ofGb(4).getBytes(), 2))
                    .add(buildNode("ml-node-without-room", true, 1000L, 2))
                    .add(buildNode("not-ml-node", false, ByteSizeValue.ofGb(4).getBytes(), 2))
                    .add(buildNode("ml-node-shutting-down", true, ByteSizeValue.ofGb(4).getBytes(), 2))
                    .add(buildOldNode("old-ml-node-with-room", true, ByteSizeValue.ofGb(4).getBytes(), 2))
                    .build()
            )
            .metadata(Metadata.builder().putCustom(NodesShutdownMetadata.TYPE, shutdownMetadata("ml-node-shutting-down")))
            .build();

        TrainedModelAssignmentClusterService trainedModelAssignmentClusterService = createClusterService();
        ClusterState newState = trainedModelAssignmentClusterService.createModelAssignment(currentState, newParams("new-model", 150, 4, 1));
        TrainedModelAssignment createdAssignment = TrainedModelAssignmentMetadata.fromState(newState).getModelAssignment("new-model");

        assertThat(createdAssignment, is(not(nullValue())));
        assertThat(createdAssignment.getNodeRoutingTable().keySet(), hasSize(1));
        assertThat(createdAssignment.getNodeRoutingTable(), hasKey("ml-node-with-room"));
        assertThat(createdAssignment.getNodeRoutingTable().get("ml-node-with-room").getState(), equalTo(RoutingState.STARTING));
        assertThat(createdAssignment.getReason().isPresent(), is(true));
        assertThat(
            createdAssignment.getReason().get(),
            containsString("Could not assign (more) allocations on node [ml-node-without-room]")
        );
        assertThat(createdAssignment.getAssignmentState(), equalTo(AssignmentState.STARTING));

        expectThrows(
            ResourceAlreadyExistsException.class,
            () -> trainedModelAssignmentClusterService.createModelAssignment(newState, newParams("new-model", 150))
        );
    }

    public void testCreateAssignmentWhileResetModeIsTrue() throws InterruptedException {
        ClusterState currentState = ClusterState.builder(new ClusterName("testCreateAssignment"))
            .nodes(DiscoveryNodes.builder().add(buildNode("ml-node-with-room", true, ByteSizeValue.ofGb(4).getBytes(), 8)).build())
            .metadata(Metadata.builder().putCustom(MlMetadata.TYPE, new MlMetadata.Builder().isResetMode(true).build()))
            .build();
        when(clusterService.state()).thenReturn(currentState);
        TrainedModelAssignmentClusterService trainedModelAssignmentClusterService = createClusterService();

        CountDownLatch latch = new CountDownLatch(1);
        trainedModelAssignmentClusterService.createNewModelAssignment(
            newParams("new-model", 150),
            new LatchedActionListener<>(
                ActionListener.wrap(
                    trainedModelAssignment -> fail("assignment should have failed to be created because reset mode is set"),
                    e -> {
                        assertThat(e, is(instanceOf(ElasticsearchStatusException.class)));
                        assertThat(((ElasticsearchStatusException) e).status(), equalTo(RestStatus.CONFLICT));
                        assertThat(
                            e.getMessage(),
                            equalTo("cannot create new assignment for model [new-model] while feature reset is in progress.")
                        );
                    }
                ),
                latch
            )
        );
        latch.await();
    }

    public void testShouldRebalanceModels() {
        String model1 = "model-1";
        String model2 = "model-2";
        String mlNode1 = "ml-node-with-room";
        String mlNode2 = "new-ml-node-with-room";
        DiscoveryNode mlNode1Node = buildNode(mlNode1, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode mlNode2Node = buildNode(mlNode2, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        ClusterState stateWithTwoNodes = ClusterState.builder(new ClusterName("testShouldAllocateModels"))
            .nodes(DiscoveryNodes.builder().add(mlNode1Node).add(mlNode2Node))
            .build();
        ClusterState stateWithOneNode = ClusterState.builder(new ClusterName("testShouldAllocateModels"))
            .nodes(DiscoveryNodes.builder().add(mlNode1Node))
            .build();
        ClusterState stateWithOneNodeNotMl = ClusterState.builder(new ClusterName("testShouldAllocateModels"))
            .nodes(DiscoveryNodes.builder().add(mlNode1Node).add(buildNode("not-ml-node", false, ByteSizeValue.ofGb(4).getBytes(), 8)))
            .build();

        // No metadata in the new state means no allocations, so no updates
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(randomFrom(stateWithOneNodeNotMl, stateWithOneNode, stateWithTwoNodes)).build(),
                    ClusterState.builder(randomFrom(stateWithOneNodeNotMl, stateWithOneNode, stateWithTwoNodes))
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build()
                )
            ),
            is(false)
        );

        // Even with metadata changes, unless there are node changes, do nothing
        ClusterState randomState = randomFrom(stateWithOneNodeNotMl, stateWithOneNode, stateWithTwoNodes);
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(randomState)
                        .metadata(
                            Metadata.builder()
                                .putCustom(TrainedModelAssignmentMetadata.NAME, TrainedModelAssignmentMetadataTests.randomInstance())
                                .build()
                        )
                        .build(),
                    ClusterState.builder(randomState)
                        .metadata(
                            Metadata.builder()
                                .putCustom(TrainedModelAssignmentMetadata.NAME, TrainedModelAssignmentMetadataTests.randomInstance())
                                .build()
                        )
                        .build()
                )
            ),
            is(false)
        );

        // If the node removed is not even an ML node, we should not attempt to re-allocate
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(stateWithOneNode)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build(),
                    ClusterState.builder(stateWithOneNodeNotMl)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build()
                )
            ),
            is(false)
        );

        // If the node removed is an ML node, but no models are allocated to it, we should not attempt to re-allocate
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(stateWithOneNode)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build(),
                    ClusterState.builder(stateWithTwoNodes)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build()
                )
            ),
            is(false)
        );

        // If a new ML node is added, we should attempt to re-allocate
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(stateWithTwoNodes)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build(),
                    ClusterState.builder(stateWithOneNode)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build()
                )
            ),
            is(true)
        );

        // If a new ML node is added, but allocation is stopping, we should not re-allocate
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(stateWithTwoNodes)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(
                                            model1,
                                            TrainedModelAssignment.Builder.empty(newParams(model1, 100)).stopAssignment("test")
                                        )
                                        .build()
                                )
                                .build()
                        )
                        .build(),
                    ClusterState.builder(stateWithOneNode)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build()
                )
            ),
            is(false)
        );

        // If a new ML node is added, but its shutting down, don't re-allocate
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(stateWithTwoNodes)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .putCustom(NodesShutdownMetadata.TYPE, shutdownMetadata(mlNode2))
                                .build()
                        )
                        .build(),
                    ClusterState.builder(stateWithOneNode)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(model1, TrainedModelAssignment.Builder.empty(newParams(model1, 100)))
                                        .build()
                                )
                                .build()
                        )
                        .build()
                )
            ),
            is(false)
        );

        // If a ML node is removed and its routed to, re-allocate
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(stateWithOneNode)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(
                                            model1,
                                            TrainedModelAssignment.Builder.empty(newParams(model1, 100))
                                                .addRoutingEntry(mlNode1, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                        )
                                        .addNewAssignment(
                                            model2,
                                            TrainedModelAssignment.Builder.empty(newParams("model-2", 100))
                                                .addRoutingEntry(mlNode1, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                                .addRoutingEntry(mlNode2, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                        )
                                        .build()
                                )
                                .build()
                        )
                        .build(),
                    ClusterState.builder(stateWithTwoNodes)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(
                                            model1,
                                            TrainedModelAssignment.Builder.empty(newParams(model1, 100))
                                                .addRoutingEntry(mlNode1, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                        )
                                        .addNewAssignment(
                                            model2,
                                            TrainedModelAssignment.Builder.empty(newParams("model-2", 100))
                                                .addRoutingEntry(mlNode1, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                                .addRoutingEntry(mlNode2, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                        )
                                        .build()
                                )
                                .build()
                        )
                        .build()
                )
            ),
            is(true)
        );

        // If a ML node is removed and its routed to, but the allocation is stopping, don't re-allocate
        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(
                new ClusterChangedEvent(
                    "test",
                    ClusterState.builder(stateWithOneNode)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(
                                            model1,
                                            TrainedModelAssignment.Builder.empty(newParams(model1, 100))
                                                .addRoutingEntry(mlNode1, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                        )
                                        .addNewAssignment(
                                            model2,
                                            TrainedModelAssignment.Builder.empty(newParams("model-2", 100))
                                                .addRoutingEntry(mlNode1, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                                .addRoutingEntry(mlNode2, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                                .stopAssignment("test")
                                        )
                                        .build()
                                )
                                .build()
                        )
                        .build(),
                    ClusterState.builder(stateWithTwoNodes)
                        .metadata(
                            Metadata.builder()
                                .putCustom(
                                    TrainedModelAssignmentMetadata.NAME,
                                    TrainedModelAssignmentMetadata.Builder.empty()
                                        .addNewAssignment(
                                            model1,
                                            TrainedModelAssignment.Builder.empty(newParams(model1, 100))
                                                .addRoutingEntry(mlNode1, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                        )
                                        .addNewAssignment(
                                            model2,
                                            TrainedModelAssignment.Builder.empty(newParams("model-2", 100))
                                                .addRoutingEntry(mlNode1, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                                .addRoutingEntry(mlNode2, new RoutingInfo(1, 1, RoutingState.STARTING, ""))
                                        )
                                        .build()
                                )
                                .build()
                        )
                        .build()
                )
            ),
            is(false)
        );
    }

    public void testShouldRebalanceModels_WithNodeShutdowns() {
        String clusterName = "testShouldAllocateModels_WithNodeShutdowns";
        String model1 = "model-1";
        DiscoveryNode mlNode1 = buildNode("ml-node-1", true, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode mlNode2 = buildNode("ml-node-2", true, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode esNode1 = buildNode("es-node-1", false, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode esNode2 = buildNode("es-node-2", false, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode esNode3 = buildNode("es-node-3", false, ByteSizeValue.ofGb(4).getBytes(), 8);

        TrainedModelAssignmentMetadata fullModelAllocation = TrainedModelAssignmentMetadata.Builder.empty()
            .addNewAssignment(
                model1,
                TrainedModelAssignment.Builder.empty(newParams(model1, 100))
                    .addRoutingEntry(mlNode1.getId(), new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                    .addRoutingEntry(mlNode2.getId(), new RoutingInfo(1, 1, RoutingState.STARTED, ""))
            )
            .build();

        ClusterState fullyAllocated = csBuilderWithNodes(clusterName, mlNode1, mlNode2, esNode1, esNode2, esNode3).metadata(
            Metadata.builder().putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation).build()
        ).build();

        // reallocate when the node is marked for shutdown
        var previousState = fullyAllocated;
        var currentState = ClusterState.builder(fullyAllocated)
            .metadata(
                Metadata.builder()
                    .putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation)
                    .putCustom(NodesShutdownMetadata.TYPE, nodesShutdownMetadata(mlNode1))
                    .build()
            )
            .build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(true)
        );

        previousState = currentState;

        // mlNode1 node is now removed we but we have already
        // reallocated on the node shutdown change
        currentState = csBuilderWithNodes(clusterName, mlNode2, esNode1, esNode2, esNode3).metadata(
            Metadata.builder()
                .putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation)
                .putCustom(NodesShutdownMetadata.TYPE, nodesShutdownMetadata(mlNode1))
                .build()
        ).build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(false)
        );

        previousState = currentState;

        // mlNode1 has returned but is still marked as shutdown
        currentState = csBuilderWithNodes(clusterName, mlNode1, mlNode2, esNode1, esNode2, esNode3).metadata(
            Metadata.builder()
                .putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation)
                .putCustom(NodesShutdownMetadata.TYPE, nodesShutdownMetadata(mlNode1))
                .build()
        ).build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(false)
        );

        previousState = currentState;

        // mlNode1 no longer marked for shutdown
        currentState = csBuilderWithNodes(clusterName, mlNode1, mlNode2, esNode1, esNode2, esNode3).metadata(
            Metadata.builder().putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation).build()
        ).build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(true)
        );

        previousState = currentState;

        // now an ES node is marked for shutdown
        currentState = csBuilderWithNodes(clusterName, mlNode1, mlNode2, esNode1, esNode2, esNode3).metadata(
            Metadata.builder()
                .putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation)
                .putCustom(NodesShutdownMetadata.TYPE, nodesShutdownMetadata(esNode1))
                .build()
        ).build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(false)
        );

        previousState = currentState;

        // The ES node is removed
        currentState = csBuilderWithNodes(clusterName, mlNode1, mlNode2, esNode2, esNode3).metadata(
            Metadata.builder()
                .putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation)
                .putCustom(NodesShutdownMetadata.TYPE, nodesShutdownMetadata(esNode1))
                .build()
        ).build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(false)
        );

        previousState = currentState;

        // The ES node returns
        currentState = csBuilderWithNodes(clusterName, mlNode1, mlNode2, esNode1, esNode2, esNode3).metadata(
            Metadata.builder()
                .putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation)
                .putCustom(NodesShutdownMetadata.TYPE, nodesShutdownMetadata(esNode1))
                .build()
        ).build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(false)
        );

        previousState = currentState;

        // The ES node is no longer marked as shutdown
        currentState = csBuilderWithNodes(clusterName, mlNode1, mlNode2, esNode1, esNode2, esNode3).metadata(
            Metadata.builder()
                .putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation)
                .putCustom(NodesShutdownMetadata.TYPE, nodesShutdownMetadata(esNode1))
                .build()
        ).build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(false)
        );

        // shutdown and node removed in the same event
        previousState = fullyAllocated;
        currentState = csBuilderWithNodes(clusterName, mlNode2, esNode1, esNode2, esNode3).metadata(
            Metadata.builder()
                .putCustom(TrainedModelAssignmentMetadata.NAME, fullModelAllocation)
                .putCustom(NodesShutdownMetadata.TYPE, nodesShutdownMetadata(mlNode1))
                .build()
        ).build();

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(true)
        );

        previousState = currentState;

        // node comes back and the shutdown is removed
        currentState = fullyAllocated;

        assertThat(
            TrainedModelAssignmentClusterService.shouldRebalanceModels(new ClusterChangedEvent("test", currentState, previousState)),
            is(true)
        );
    }

    public void testAreAssignedNodesRemoved_GivenRemovedNodeThatIsRouted() {
        String modelId = "existing-model";
        String nodeId1 = "node-1";
        String nodeId2 = "node-2";
        Metadata metadata = Metadata.builder()
            .putCustom(
                TrainedModelAssignmentMetadata.NAME,
                TrainedModelAssignmentMetadata.Builder.empty()
                    .addNewAssignment(
                        modelId,
                        TrainedModelAssignment.Builder.empty(newParams(modelId, 10_000L))
                            .addRoutingEntry(nodeId1, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                            .addRoutingEntry(nodeId2, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                    )
                    .build()
            )
            .build();
        DiscoveryNode node1 = buildNode(nodeId1, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode node2 = buildNode(nodeId2, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        ClusterState previousState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).add(node2).build())
            .metadata(metadata)
            .build();
        ClusterState currentState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).build())
            .metadata(metadata)
            .build();
        ClusterChangedEvent event = new ClusterChangedEvent("test", currentState, previousState);

        assertThat(TrainedModelAssignmentClusterService.areAssignedNodesRemoved(event), is(true));
    }

    public void testAreAssignedNodesRemoved_GivenRemovedNodeThatIsNotRouted() {
        String modelId = "existing-model";
        String nodeId1 = "node-1";
        String nodeId2 = "node-2";
        Metadata metadata = Metadata.builder()
            .putCustom(
                TrainedModelAssignmentMetadata.NAME,
                TrainedModelAssignmentMetadata.Builder.empty()
                    .addNewAssignment(
                        modelId,
                        TrainedModelAssignment.Builder.empty(newParams(modelId, 10_000L))
                            .addRoutingEntry(nodeId1, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                    )
                    .build()
            )
            .build();
        DiscoveryNode node1 = buildNode(nodeId1, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode node2 = buildNode(nodeId2, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        ClusterState previousState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).add(node2).build())
            .metadata(metadata)
            .build();
        ClusterState currentState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).build())
            .metadata(metadata)
            .build();
        ClusterChangedEvent event = new ClusterChangedEvent("test", currentState, previousState);

        assertThat(TrainedModelAssignmentClusterService.areAssignedNodesRemoved(event), is(false));
    }

    public void testAreAssignedNodesRemoved_GivenShuttingDownNodeThatIsRouted() {
        String modelId = "existing-model";
        String nodeId1 = "node-1";
        String nodeId2 = "node-2";
        TrainedModelAssignmentMetadata trainedModelAssignmentMetadata = TrainedModelAssignmentMetadata.Builder.empty()
            .addNewAssignment(
                modelId,
                TrainedModelAssignment.Builder.empty(newParams(modelId, 10_000L))
                    .addRoutingEntry(nodeId1, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                    .addRoutingEntry(nodeId2, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
            )
            .build();
        DiscoveryNode node1 = buildNode(nodeId1, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode node2 = buildNode(nodeId2, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        ClusterState previousState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).add(node2).build())
            .metadata(Metadata.builder().putCustom(TrainedModelAssignmentMetadata.NAME, trainedModelAssignmentMetadata))
            .build();
        ClusterState currentState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).add(node2).build())
            .metadata(
                Metadata.builder()
                    .putCustom(TrainedModelAssignmentMetadata.NAME, trainedModelAssignmentMetadata)
                    .putCustom(
                        NodesShutdownMetadata.TYPE,
                        new NodesShutdownMetadata(
                            Map.of(
                                nodeId1,
                                SingleNodeShutdownMetadata.builder()
                                    .setNodeId(nodeId1)
                                    .setType(SingleNodeShutdownMetadata.Type.REMOVE)
                                    .setStartedAtMillis(System.currentTimeMillis())
                                    .setReason("test")
                                    .build()
                            )
                        )
                    )
            )
            .build();
        ClusterChangedEvent event = new ClusterChangedEvent("test", currentState, previousState);

        assertThat(TrainedModelAssignmentClusterService.areAssignedNodesRemoved(event), is(true));
    }

    public void testAreAssignedNodesRemoved_GivenShuttingDownNodeThatIsNotRouted() {
        String modelId = "existing-model";
        String nodeId1 = "node-1";
        String nodeId2 = "node-2";
        TrainedModelAssignmentMetadata trainedModelAssignmentMetadata = TrainedModelAssignmentMetadata.Builder.empty()
            .addNewAssignment(
                modelId,
                TrainedModelAssignment.Builder.empty(newParams(modelId, 10_000L))
                    .addRoutingEntry(nodeId2, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
            )
            .build();
        DiscoveryNode node1 = buildNode(nodeId1, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode node2 = buildNode(nodeId2, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        ClusterState previousState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).add(node2).build())
            .metadata(Metadata.builder().putCustom(TrainedModelAssignmentMetadata.NAME, trainedModelAssignmentMetadata))
            .build();
        ClusterState currentState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).add(node2).build())
            .metadata(
                Metadata.builder()
                    .putCustom(TrainedModelAssignmentMetadata.NAME, trainedModelAssignmentMetadata)
                    .putCustom(
                        NodesShutdownMetadata.TYPE,
                        new NodesShutdownMetadata(
                            Map.of(
                                nodeId1,
                                SingleNodeShutdownMetadata.builder()
                                    .setNodeId(nodeId1)
                                    .setType(SingleNodeShutdownMetadata.Type.REMOVE)
                                    .setStartedAtMillis(System.currentTimeMillis())
                                    .setReason("test")
                                    .build()
                            )
                        )
                    )
            )
            .build();
        ClusterChangedEvent event = new ClusterChangedEvent("test", currentState, previousState);

        assertThat(TrainedModelAssignmentClusterService.areAssignedNodesRemoved(event), is(false));
    }

    public void testRemoveRoutingToUnassignableNodes() {
        String modelId1 = "model-1";
        String modelId2 = "model-2";
        String nodeId1 = "node-1";
        String nodeId2 = "node-2";
        String nodeId3 = "node-3";
        Metadata metadata = Metadata.builder()
            .putCustom(
                TrainedModelAssignmentMetadata.NAME,
                TrainedModelAssignmentMetadata.Builder.empty()
                    .addNewAssignment(
                        modelId1,
                        TrainedModelAssignment.Builder.empty(newParams(modelId1, 10_000L))
                            .addRoutingEntry(nodeId1, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                            .addRoutingEntry(nodeId2, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                            .addRoutingEntry(nodeId3, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                    )
                    .addNewAssignment(
                        modelId2,
                        TrainedModelAssignment.Builder.empty(newParams(modelId2, 10_000L))
                            .addRoutingEntry(nodeId1, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                            .addRoutingEntry(nodeId2, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                            .addRoutingEntry(nodeId3, new RoutingInfo(1, 1, RoutingState.STARTED, ""))
                    )
                    .build()
            )
            .putCustom(
                NodesShutdownMetadata.TYPE,
                new NodesShutdownMetadata(
                    Map.of(
                        nodeId3,
                        SingleNodeShutdownMetadata.builder()
                            .setNodeId(nodeId3)
                            .setType(SingleNodeShutdownMetadata.Type.REMOVE)
                            .setStartedAtMillis(System.currentTimeMillis())
                            .setReason("test")
                            .build()
                    )
                )
            )
            .build();
        DiscoveryNode node1 = buildNode(nodeId1, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        DiscoveryNode node3 = buildNode(nodeId3, true, ByteSizeValue.ofGb(4).getBytes(), 8);
        ClusterState currentState = ClusterState.builder(new ClusterName("testAreAssignedNodesRemoved"))
            .nodes(DiscoveryNodes.builder().add(node1).add(node3).build())
            .metadata(metadata)
            .build();

        ClusterState resultState = TrainedModelAssignmentClusterService.removeRoutingToUnassignableNodes(currentState);

        TrainedModelAssignmentMetadata trainedModelAssignmentMetadata = TrainedModelAssignmentMetadata.fromState(resultState);
        assertThat(trainedModelAssignmentMetadata.modelAssignments(), is(aMapWithSize(2)));
        for (String modelId : List.of(modelId1, modelId2)) {
            TrainedModelAssignment assignment = trainedModelAssignmentMetadata.getModelAssignment(modelId);
            assertThat(assignment, is(notNullValue()));
            assertThat(assignment.getNodeRoutingTable(), is(aMapWithSize(1)));
            assertThat(assignment.getNodeRoutingTable(), hasKey(nodeId1));
        }
    }

    private ClusterState.Builder csBuilderWithNodes(String name, DiscoveryNode... nodes) {
        var csBuilder = ClusterState.builder(new ClusterName(name));
        var nodeBuilder = DiscoveryNodes.builder();
        for (var node : nodes) {
            nodeBuilder.add(node);
        }
        csBuilder.nodes(nodeBuilder);
        return csBuilder;
    }

    private NodesShutdownMetadata nodesShutdownMetadata(DiscoveryNode nodeToShutdown) {
        return new NodesShutdownMetadata(
            Map.of(
                nodeToShutdown.getId(),
                SingleNodeShutdownMetadata.builder()
                    .setNodeId(nodeToShutdown.getId())
                    .setStartedAtMillis(1L)
                    .setType(SingleNodeShutdownMetadata.Type.RESTART)
                    .setReason("because this cannot be null")
                    .build()
            )
        );
    }

    public void testSetAllocationToStopping() {
        ClusterState clusterStateWithoutAllocation = ClusterState.builder(new ClusterName("testSetAllocationToStopping"))
            .metadata(Metadata.builder().build())
            .build();
        String modelId = "stopping-allocation";

        expectThrows(
            ResourceNotFoundException.class,
            () -> TrainedModelAssignmentClusterService.setToStopping(clusterStateWithoutAllocation, modelId, "test")
        );

        ClusterState clusterStateWithAllocation = ClusterState.builder(new ClusterName("testSetAllocationToStopping"))
            .nodes(DiscoveryNodes.builder().add(buildNode("test-node", true, ByteSizeValue.ofGb(4).getBytes(), 8)).build())
            .metadata(
                Metadata.builder()
                    .putCustom(
                        TrainedModelAssignmentMetadata.NAME,
                        TrainedModelAssignmentMetadata.Builder.empty()
                            .addNewAssignment(modelId, TrainedModelAssignment.Builder.empty(newParams(modelId, randomNonNegativeLong())))
                            .build()
                    )
                    .build()
            )
            .build();
        TrainedModelAssignmentMetadata before = TrainedModelAssignmentMetadata.fromState(clusterStateWithAllocation);
        assertThat(before.getModelAssignment(modelId), is(not(nullValue())));
        assertThat(before.getModelAssignment(modelId).getAssignmentState(), equalTo(AssignmentState.STARTING));

        ClusterState modified = TrainedModelAssignmentClusterService.setToStopping(clusterStateWithAllocation, modelId, "test");
        assertThat(
            TrainedModelAssignmentMetadata.fromState(modified).getModelAssignment(modelId).getAssignmentState(),
            equalTo(AssignmentState.STOPPING)
        );
    }

    private void assertThatStoppingAssignmentPreventsMutation(
        Function<ClusterState, ClusterState> mutationFunction,
        ClusterState original
    ) {
        TrainedModelAssignmentMetadata tempMetadata = TrainedModelAssignmentMetadata.fromState(original);
        if (tempMetadata.modelAssignments().isEmpty()) {
            return;
        }
        TrainedModelAssignmentMetadata.Builder builder = TrainedModelAssignmentMetadata.builder(original);
        for (String modelId : tempMetadata.modelAssignments().keySet()) {
            builder.getAssignment(modelId).stopAssignment("test");
        }
        TrainedModelAssignmentMetadata metadataWithStopping = builder.build();
        ClusterState originalWithStoppingAllocations = ClusterState.builder(original)
            .metadata(Metadata.builder(original.metadata()).putCustom(TrainedModelAssignmentMetadata.NAME, metadataWithStopping).build())
            .build();

        assertThat(
            "setting all allocations to stopping did not prevent mutation",
            TrainedModelAssignmentMetadata.fromState(mutationFunction.apply(originalWithStoppingAllocations)),
            equalTo(metadataWithStopping)
        );
    }

    private TrainedModelAssignmentClusterService createClusterService() {
        return new TrainedModelAssignmentClusterService(Settings.EMPTY, clusterService, threadPool, nodeLoadDetector);
    }

    private static DiscoveryNode buildNode(String name, boolean isML, long nativeMemory, int allocatedProcessors) {
        return buildNode(name, isML, nativeMemory, allocatedProcessors, Version.CURRENT);
    }

    private static DiscoveryNode buildNode(String name, boolean isML, long nativeMemory, int allocatedProcessors, Version version) {
        return new DiscoveryNode(
            name,
            name,
            buildNewFakeTransportAddress(),
            MapBuilder.<String, String>newMapBuilder()
                .put(MachineLearning.MACHINE_MEMORY_NODE_ATTR, String.valueOf(nativeMemory))
                .put(MachineLearning.MAX_JVM_SIZE_NODE_ATTR, String.valueOf(10))
                .put(MachineLearning.ALLOCATED_PROCESSORS_NODE_ATTR, String.valueOf(allocatedProcessors))
                .map(),
            isML ? DiscoveryNodeRole.roles() : Set.of(DiscoveryNodeRole.DATA_ROLE, DiscoveryNodeRole.MASTER_ROLE),
            version
        );
    }

    private static RoutingInfoUpdate started() {
        return RoutingInfoUpdate.updateStateAndReason(new RoutingStateAndReason(RoutingState.STARTED, ""));
    }

    private static DiscoveryNode buildOldNode(String name, boolean isML, long nativeMemory, int allocatedProcessors) {
        return buildNode(name, isML, nativeMemory, allocatedProcessors, Version.V_7_15_0);
    }

    private static StartTrainedModelDeploymentAction.TaskParams newParams(String modelId, long modelSize) {
        return newParams(modelId, modelSize, 1, 1);
    }

    private static StartTrainedModelDeploymentAction.TaskParams newParams(
        String modelId,
        long modelSize,
        int numberOfAllocations,
        int threadsPerAllocation
    ) {
        return new StartTrainedModelDeploymentAction.TaskParams(modelId, modelSize, threadsPerAllocation, numberOfAllocations, 1024);
    }

    private static NodesShutdownMetadata shutdownMetadata(String nodeId) {
        return new NodesShutdownMetadata(
            Collections.singletonMap(
                nodeId,
                SingleNodeShutdownMetadata.builder()
                    .setType(SingleNodeShutdownMetadata.Type.REMOVE)
                    .setStartedAtMillis(randomNonNegativeLong())
                    .setReason("tests")
                    .setNodeId(nodeId)
                    .build()
            )
        );
    }
}
