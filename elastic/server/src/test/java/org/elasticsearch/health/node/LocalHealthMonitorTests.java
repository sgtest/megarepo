/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.health.node;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.action.support.replication.ClusterStateCreationUtils;
import org.elasticsearch.client.internal.Client;
import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.node.DiscoveryNode;
import org.elasticsearch.cluster.node.DiscoveryNodeRole;
import org.elasticsearch.cluster.node.DiscoveryNodeUtils;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.RelativeByteSizeValue;
import org.elasticsearch.core.TimeValue;
import org.elasticsearch.features.FeatureService;
import org.elasticsearch.health.HealthFeatures;
import org.elasticsearch.health.HealthStatus;
import org.elasticsearch.health.metadata.HealthMetadata;
import org.elasticsearch.health.node.tracker.HealthTracker;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.junit.AfterClass;
import org.junit.Before;
import org.junit.BeforeClass;

import java.util.List;
import java.util.Set;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class LocalHealthMonitorTests extends ESTestCase {

    private static final DiskHealthInfo GREEN = new DiskHealthInfo(HealthStatus.GREEN, null);
    private static ThreadPool threadPool;
    private ClusterService clusterService;
    private DiscoveryNode node;
    private DiscoveryNode frozenNode;
    private HealthMetadata healthMetadata;
    private ClusterState clusterState;
    private Client client;
    private MockHealthTracker mockHealthTracker;
    private LocalHealthMonitor localHealthMonitor;

    @BeforeClass
    public static void setUpThreadPool() {
        threadPool = new TestThreadPool(LocalHealthMonitorTests.class.getSimpleName());
    }

    @AfterClass
    public static void tearDownThreadPool() {
        terminate(threadPool);
    }

    @Before
    @SuppressWarnings("unchecked")
    public void setUp() throws Exception {
        super.setUp();
        // Set-up cluster state
        healthMetadata = new HealthMetadata(
            HealthMetadata.Disk.newBuilder()
                .highWatermark(new RelativeByteSizeValue(ByteSizeValue.ofBytes(100)))
                .floodStageWatermark(new RelativeByteSizeValue(ByteSizeValue.ofBytes(50)))
                .frozenFloodStageWatermark(new RelativeByteSizeValue(ByteSizeValue.ofBytes(50)))
                .frozenFloodStageMaxHeadroom(ByteSizeValue.ofBytes(10))
                .build(),
            HealthMetadata.ShardLimits.newBuilder().maxShardsPerNode(999).maxShardsPerNodeFrozen(100).build()
        );
        node = DiscoveryNodeUtils.create("node", "node");
        frozenNode = DiscoveryNodeUtils.builder("frozen-node")
            .name("frozen-node")
            .roles(Set.of(DiscoveryNodeRole.DATA_FROZEN_NODE_ROLE))
            .build();
        var searchNode = DiscoveryNodeUtils.builder("search-node").name("search-node").roles(Set.of(DiscoveryNodeRole.SEARCH_ROLE)).build();
        var searchAndIndexNode = DiscoveryNodeUtils.builder("search-and-index-node")
            .name("search-and-index-node")
            .roles(Set.of(DiscoveryNodeRole.SEARCH_ROLE, DiscoveryNodeRole.INDEX_ROLE))
            .build();
        clusterState = ClusterStateCreationUtils.state(
            node,
            node,
            node,
            new DiscoveryNode[] { node, frozenNode, searchNode, searchAndIndexNode }
        ).copyAndUpdate(b -> b.putCustom(HealthMetadata.TYPE, healthMetadata));

        // Set-up cluster service
        clusterService = mock(ClusterService.class);
        when(clusterService.getClusterSettings()).thenReturn(
            new ClusterSettings(Settings.EMPTY, ClusterSettings.BUILT_IN_CLUSTER_SETTINGS)
        );
        when(clusterService.state()).thenReturn(clusterState);
        when(clusterService.localNode()).thenReturn(node);

        // Set-up node service with a node with a healthy disk space usage

        client = mock(Client.class);

        FeatureService featureService = new FeatureService(List.of(new HealthFeatures()));

        mockHealthTracker = new MockHealthTracker();

        localHealthMonitor = LocalHealthMonitor.create(
            Settings.EMPTY,
            clusterService,
            threadPool,
            client,
            featureService,
            List.of(mockHealthTracker)
        );
    }

    @SuppressWarnings("unchecked")
    public void testUpdateHealthInfo() throws Exception {
        doAnswer(invocation -> {
            DiskHealthInfo diskHealthInfo = ((UpdateHealthInfoCacheAction.Request) invocation.getArgument(1)).getDiskHealthInfo();
            ActionListener<AcknowledgedResponse> listener = (ActionListener<AcknowledgedResponse>) invocation.getArguments()[2];
            assertThat(diskHealthInfo, equalTo(GREEN));
            listener.onResponse(null);
            return null;
        }).when(client).execute(any(), any(), any());

        // We override the poll interval like this to avoid the min value set by the setting which is too high for this test
        localHealthMonitor.setMonitorInterval(TimeValue.timeValueMillis(10));
        assertThat(mockHealthTracker.getLastReportedValue(), nullValue());
        localHealthMonitor.clusterChanged(new ClusterChangedEvent("initialize", clusterState, ClusterState.EMPTY_STATE));
        assertBusy(() -> assertThat(mockHealthTracker.getLastReportedValue(), equalTo(GREEN)));
    }

    @SuppressWarnings("unchecked")
    public void testDoNotUpdateHealthInfoOnFailure() throws Exception {
        AtomicReference<Boolean> clientCalled = new AtomicReference<>(false);
        doAnswer(invocation -> {
            ActionListener<AcknowledgedResponse> listener = (ActionListener<AcknowledgedResponse>) invocation.getArguments()[2];
            listener.onFailure(new RuntimeException("simulated"));
            clientCalled.set(true);
            return null;
        }).when(client).execute(any(), any(), any());

        localHealthMonitor.clusterChanged(new ClusterChangedEvent("initialize", clusterState, ClusterState.EMPTY_STATE));
        assertBusy(() -> assertThat(clientCalled.get(), equalTo(true)));
        assertThat(mockHealthTracker.getLastReportedValue(), nullValue());
    }

    @SuppressWarnings("unchecked")
    public void testSendHealthInfoToNewNode() throws Exception {
        ClusterState previous = ClusterStateCreationUtils.state(node, node, frozenNode, new DiscoveryNode[] { node, frozenNode })
            .copyAndUpdate(b -> b.putCustom(HealthMetadata.TYPE, healthMetadata));
        ClusterState current = ClusterStateCreationUtils.state(node, node, node, new DiscoveryNode[] { node, frozenNode })
            .copyAndUpdate(b -> b.putCustom(HealthMetadata.TYPE, healthMetadata));

        AtomicInteger counter = new AtomicInteger(0);
        doAnswer(invocation -> {
            DiskHealthInfo diskHealthInfo = ((UpdateHealthInfoCacheAction.Request) invocation.getArgument(1)).getDiskHealthInfo();
            ActionListener<AcknowledgedResponse> listener = (ActionListener<AcknowledgedResponse>) invocation.getArguments()[2];
            assertThat(diskHealthInfo, equalTo(GREEN));
            counter.incrementAndGet();
            listener.onResponse(null);
            return null;
        }).when(client).execute(any(), any(), any());

        when(clusterService.state()).thenReturn(previous);
        localHealthMonitor.clusterChanged(new ClusterChangedEvent("start-up", previous, ClusterState.EMPTY_STATE));
        assertBusy(() -> assertThat(mockHealthTracker.getLastReportedValue(), equalTo(GREEN)));
        localHealthMonitor.clusterChanged(new ClusterChangedEvent("health-node-switch", current, previous));
        assertBusy(() -> assertThat(counter.get(), equalTo(2)));
    }

    @SuppressWarnings("unchecked")
    public void testResendHealthInfoOnMasterChange() throws Exception {
        ClusterState previous = ClusterStateCreationUtils.state(node, node, node, new DiscoveryNode[] { node, frozenNode })
            .copyAndUpdate(b -> b.putCustom(HealthMetadata.TYPE, healthMetadata));
        ClusterState current = ClusterStateCreationUtils.state(node, frozenNode, node, new DiscoveryNode[] { node, frozenNode })
            .copyAndUpdate(b -> b.putCustom(HealthMetadata.TYPE, healthMetadata));

        AtomicInteger counter = new AtomicInteger(0);
        doAnswer(invocation -> {
            DiskHealthInfo diskHealthInfo = ((UpdateHealthInfoCacheAction.Request) invocation.getArgument(1)).getDiskHealthInfo();
            ActionListener<AcknowledgedResponse> listener = (ActionListener<AcknowledgedResponse>) invocation.getArguments()[2];
            assertThat(diskHealthInfo, equalTo(GREEN));
            counter.incrementAndGet();
            listener.onResponse(null);
            return null;
        }).when(client).execute(any(), any(), any());

        when(clusterService.state()).thenReturn(previous);
        localHealthMonitor.clusterChanged(new ClusterChangedEvent("start-up", previous, ClusterState.EMPTY_STATE));
        assertBusy(() -> assertThat(mockHealthTracker.getLastReportedValue(), equalTo(GREEN)));
        localHealthMonitor.clusterChanged(new ClusterChangedEvent("health-node-switch", current, previous));
        assertBusy(() -> assertThat(counter.get(), equalTo(2)));
    }

    @SuppressWarnings("unchecked")
    public void testEnablingAndDisabling() throws Exception {
        AtomicInteger clientCalledCount = new AtomicInteger();
        doAnswer(invocation -> {
            ActionListener<AcknowledgedResponse> listener = (ActionListener<AcknowledgedResponse>) invocation.getArguments()[2];
            clientCalledCount.incrementAndGet();
            listener.onResponse(null);
            return null;
        }).when(client).execute(any(), any(), any());
        when(clusterService.state()).thenReturn(null);

        // Ensure that there are no issues if the cluster state hasn't been initialized yet
        localHealthMonitor.setEnabled(true);
        assertThat(mockHealthTracker.getLastReportedValue(), nullValue());
        assertThat(clientCalledCount.get(), equalTo(0));

        when(clusterService.state()).thenReturn(clusterState);
        localHealthMonitor.clusterChanged(new ClusterChangedEvent("test", clusterState, ClusterState.EMPTY_STATE));
        assertBusy(() -> assertThat(mockHealthTracker.getLastReportedValue(), equalTo(GREEN)));
        assertThat(clientCalledCount.get(), equalTo(1));

        DiskHealthInfo nextHealthStatus = new DiskHealthInfo(HealthStatus.RED, DiskHealthInfo.Cause.NODE_OVER_THE_FLOOD_STAGE_THRESHOLD);

        // Disable the local monitoring
        localHealthMonitor.setEnabled(false);
        localHealthMonitor.setMonitorInterval(TimeValue.timeValueMillis(1));
        mockHealthTracker.setHealthInfo(nextHealthStatus);
        assertThat(clientCalledCount.get(), equalTo(1));
        localHealthMonitor.setMonitorInterval(TimeValue.timeValueSeconds(30));

        localHealthMonitor.setEnabled(true);
        assertBusy(() -> assertThat(mockHealthTracker.getLastReportedValue(), equalTo(nextHealthStatus)));
    }

    private static class MockHealthTracker extends HealthTracker<DiskHealthInfo> {
        private DiskHealthInfo healthInfo = GREEN;

        @Override
        public DiskHealthInfo checkCurrentHealth() {
            return healthInfo;
        }

        @Override
        public void addToRequestBuilder(UpdateHealthInfoCacheAction.Request.Builder builder, DiskHealthInfo healthInfo) {
            builder.diskHealthInfo(healthInfo);
        }

        public void setHealthInfo(DiskHealthInfo healthInfo) {
            this.healthInfo = healthInfo;
        }
    }
}
