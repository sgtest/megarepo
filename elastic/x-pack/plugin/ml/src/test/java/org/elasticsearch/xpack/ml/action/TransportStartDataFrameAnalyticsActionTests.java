/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.action;

import org.elasticsearch.Version;
import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexNameExpressionResolver;
import org.elasticsearch.cluster.metadata.Metadata;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodes;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.common.util.concurrent.ThreadContext;
import org.elasticsearch.common.util.set.Sets;
import org.elasticsearch.persistent.PersistentTasksCustomMetadata.Assignment;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.ml.MlMetadata;
import org.elasticsearch.xpack.core.ml.action.StartDataFrameAnalyticsAction.TaskParams;
import org.elasticsearch.xpack.ml.MachineLearning;
import org.elasticsearch.xpack.ml.action.TransportStartDataFrameAnalyticsAction.TaskExecutor;
import org.elasticsearch.xpack.ml.dataframe.DataFrameAnalyticsManager;
import org.elasticsearch.xpack.ml.notifications.DataFrameAnalyticsAuditor;
import org.elasticsearch.xpack.ml.process.MlMemoryTracker;

import java.net.InetAddress;
import java.util.Collections;
import java.util.Map;

import static org.hamcrest.Matchers.allOf;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.emptyString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class TransportStartDataFrameAnalyticsActionTests extends ESTestCase {

    private static final String JOB_ID = "data_frame_id";

    // Cannot assign the node because upgrade mode is enabled
    public void testGetAssignment_UpgradeModeIsEnabled() {
        TaskExecutor executor = createTaskExecutor();
        TaskParams params = new TaskParams(JOB_ID, Version.CURRENT, false);
        ClusterState clusterState =
            ClusterState.builder(new ClusterName("_name"))
                .metadata(Metadata.builder().putCustom(MlMetadata.TYPE, new MlMetadata.Builder().isUpgradeMode(true).build()))
                .build();

        Assignment assignment = executor.getAssignment(params, clusterState);
        assertThat(assignment.getExecutorNode(), is(nullValue()));
        assertThat(assignment.getExplanation(), is(equalTo("persistent task cannot be assigned while upgrade mode is enabled.")));
    }

    // Cannot assign the node because there are no existing nodes in the cluster state
    public void testGetAssignment_NoNodes() {
        TaskExecutor executor = createTaskExecutor();
        TaskParams params = new TaskParams(JOB_ID, Version.CURRENT, false);
        ClusterState clusterState =
            ClusterState.builder(new ClusterName("_name"))
                .metadata(Metadata.builder().putCustom(MlMetadata.TYPE, new MlMetadata.Builder().build()))
                .build();

        Assignment assignment = executor.getAssignment(params, clusterState);
        assertThat(assignment.getExecutorNode(), is(nullValue()));
        assertThat(assignment.getExplanation(), is(emptyString()));
    }

    // Cannot assign the node because none of the existing nodes is an ML node
    public void testGetAssignment_NoMlNodes() {
        TaskExecutor executor = createTaskExecutor();
        TaskParams params = new TaskParams(JOB_ID, Version.CURRENT, false);
        ClusterState clusterState =
            ClusterState.builder(new ClusterName("_name"))
                .metadata(Metadata.builder().putCustom(MlMetadata.TYPE, new MlMetadata.Builder().build()))
                .nodes(DiscoveryNodes.builder()
                    .add(createNode(0, false, Version.CURRENT))
                    .add(createNode(1, false, Version.CURRENT))
                    .add(createNode(2, false, Version.CURRENT)))
                .build();

        Assignment assignment = executor.getAssignment(params, clusterState);
        assertThat(assignment.getExecutorNode(), is(nullValue()));
        assertThat(
            assignment.getExplanation(),
            allOf(
                containsString("Not opening job [data_frame_id] on node [_node_name0], because this node isn't a ml node."),
                containsString("Not opening job [data_frame_id] on node [_node_name1], because this node isn't a ml node."),
                containsString("Not opening job [data_frame_id] on node [_node_name2], because this node isn't a ml node.")));
    }

    // Cannot assign the node because none of the existing nodes is appropriate:
    //  - _node_name0 is too old (version 7.2.0)
    //  - _node_name1 is too old (version 7.9.1)
    //  - _node_name2 is too old (version 7.9.2)
    public void testGetAssignment_MlNodesAreTooOld() {
        TaskExecutor executor = createTaskExecutor();
        TaskParams params = new TaskParams(JOB_ID, Version.CURRENT, false);
        ClusterState clusterState =
            ClusterState.builder(new ClusterName("_name"))
                .metadata(Metadata.builder().putCustom(MlMetadata.TYPE, new MlMetadata.Builder().build()))
                .nodes(DiscoveryNodes.builder()
                    .add(createNode(0, true, Version.V_7_2_0))
                    .add(createNode(1, true, Version.V_7_9_1))
                    .add(createNode(2, true, Version.V_7_9_2)))
                .build();

        Assignment assignment = executor.getAssignment(params, clusterState);
        assertThat(assignment.getExecutorNode(), is(nullValue()));
        assertThat(
            assignment.getExplanation(),
            allOf(
                containsString("Not opening job [data_frame_id] on node [{_node_name0}{version=7.2.0}], "
                    + "because the data frame analytics requires a node of version [7.3.0] or higher"),
                containsString("Not opening job [data_frame_id] on node [{_node_name1}{version=7.9.1}], "
                    + "because the data frame analytics created for version [8.0.0] requires a node of version [7.10.0] or higher"),
                containsString("Not opening job [data_frame_id] on node [{_node_name2}{version=7.9.2}], "
                    + "because the data frame analytics created for version [8.0.0] requires a node of version [7.10.0] or higher")));
    }

    // The node can be assigned despite being newer than the job.
    // In such a case destination index will be created from scratch so that its mappings are up-to-date.
    public void testGetAssignment_MlNodeIsNewerThanTheMlJobButTheAssignmentSuceeds() {
        TaskExecutor executor = createTaskExecutor();
        TaskParams params = new TaskParams(JOB_ID, Version.V_7_9_0, false);
        ClusterState clusterState =
            ClusterState.builder(new ClusterName("_name"))
                .metadata(Metadata.builder().putCustom(MlMetadata.TYPE, new MlMetadata.Builder().build()))
                .nodes(DiscoveryNodes.builder()
                    .add(createNode(0, true, Version.V_7_10_0)))
                .build();

        Assignment assignment = executor.getAssignment(params, clusterState);
        assertThat(assignment.getExecutorNode(), is(equalTo("_node_id0")));
        assertThat(assignment.getExplanation(), is(emptyString()));
    }

    private static TaskExecutor createTaskExecutor() {
        ClusterService clusterService = mock(ClusterService.class);
        ClusterSettings clusterSettings =
            new ClusterSettings(
                Settings.EMPTY,
                Sets.newHashSet(
                    MachineLearning.CONCURRENT_JOB_ALLOCATIONS,
                    MachineLearning.MAX_MACHINE_MEMORY_PERCENT,
                    MachineLearning.USE_AUTO_MACHINE_MEMORY_PERCENT,
                    MachineLearning.MAX_ML_NODE_SIZE,
                    MachineLearning.MAX_LAZY_ML_NODES,
                    MachineLearning.MAX_OPEN_JOBS_PER_NODE));
        when(clusterService.getClusterSettings()).thenReturn(clusterSettings);

        return new TaskExecutor(
            Settings.EMPTY,
            mock(Client.class),
            clusterService,
            mock(DataFrameAnalyticsManager.class),
            mock(DataFrameAnalyticsAuditor.class),
            mock(MlMemoryTracker.class),
            new IndexNameExpressionResolver(new ThreadContext(Settings.EMPTY)));
    }

    private static DiscoveryNode createNode(int i, boolean isMlNode, Version nodeVersion) {
        return new DiscoveryNode(
            "_node_name" + i,
            "_node_id" + i,
            new TransportAddress(InetAddress.getLoopbackAddress(), 9300 + i),
            Map.of("ml.max_open_jobs", isMlNode ? "10" : "0", "ml.machine_memory", "-1"),
            Collections.emptySet(),
            nodeVersion);
    }
}
