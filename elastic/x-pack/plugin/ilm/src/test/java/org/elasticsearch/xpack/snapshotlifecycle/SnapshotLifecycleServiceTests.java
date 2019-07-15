/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.snapshotlifecycle;

import org.elasticsearch.cluster.ClusterChangedEvent;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.cluster.metadata.RepositoriesMetaData;
import org.elasticsearch.cluster.metadata.RepositoryMetaData;
import org.elasticsearch.cluster.service.ClusterService;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.test.ClusterServiceUtils;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.threadpool.TestThreadPool;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.xpack.core.indexlifecycle.OperationMode;
import org.elasticsearch.xpack.core.scheduler.SchedulerEngine;
import org.elasticsearch.xpack.core.snapshotlifecycle.SnapshotLifecycleMetadata;
import org.elasticsearch.xpack.core.snapshotlifecycle.SnapshotLifecyclePolicy;
import org.elasticsearch.xpack.core.snapshotlifecycle.SnapshotLifecyclePolicyMetadata;
import org.elasticsearch.xpack.core.watcher.watch.ClockMock;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;

import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.greaterThan;

public class SnapshotLifecycleServiceTests extends ESTestCase {

    public void testGetJobId() {
        String id = randomAlphaOfLengthBetween(1, 10) + (randomBoolean() ? "" : randomLong());
        SnapshotLifecyclePolicy policy = createPolicy(id);
        long version = randomNonNegativeLong();
        SnapshotLifecyclePolicyMetadata meta = SnapshotLifecyclePolicyMetadata.builder()
            .setPolicy(policy)
            .setHeaders(Collections.emptyMap())
            .setVersion(version)
            .setModifiedDate(1)
            .build();
        assertThat(SnapshotLifecycleService.getJobId(meta), equalTo(id + "-" + version));
    }

    public void testRepositoryExistenceForExistingRepo() {
        ClusterState state = ClusterState.builder(new ClusterName("cluster")).build();

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> SnapshotLifecycleService.validateRepositoryExists("repo", state));

        assertThat(e.getMessage(), containsString("no such repository [repo]"));

        RepositoryMetaData repo = new RepositoryMetaData("repo", "fs", Settings.EMPTY);
        RepositoriesMetaData repoMeta = new RepositoriesMetaData(Collections.singletonList(repo));
        ClusterState stateWithRepo = ClusterState.builder(state)
            .metaData(MetaData.builder()
            .putCustom(RepositoriesMetaData.TYPE, repoMeta))
            .build();

        SnapshotLifecycleService.validateRepositoryExists("repo", stateWithRepo);
    }

    public void testRepositoryExistenceForMissingRepo() {
        ClusterState state = ClusterState.builder(new ClusterName("cluster")).build();

        IllegalArgumentException e = expectThrows(IllegalArgumentException.class,
            () -> SnapshotLifecycleService.validateRepositoryExists("repo", state));

        assertThat(e.getMessage(), containsString("no such repository [repo]"));
    }

    public void testNothingScheduledWhenNotRunning() {
        ClockMock clock = new ClockMock();
        SnapshotLifecyclePolicyMetadata initialPolicy = SnapshotLifecyclePolicyMetadata.builder()
            .setPolicy(createPolicy("initial", "*/1 * * * * ?"))
            .setHeaders(Collections.emptyMap())
            .setVersion(1)
            .setModifiedDate(1)
            .build();
        ClusterState initialState = createState(new SnapshotLifecycleMetadata(
            Collections.singletonMap(initialPolicy.getPolicy().getId(), initialPolicy), OperationMode.RUNNING));
        try (ThreadPool threadPool = new TestThreadPool("test");
             ClusterService clusterService = ClusterServiceUtils.createClusterService(initialState, threadPool);
             SnapshotLifecycleService sls = new SnapshotLifecycleService(Settings.EMPTY,
                 () -> new FakeSnapshotTask(e -> logger.info("triggered")), clusterService, clock)) {

            sls.offMaster();

            SnapshotLifecyclePolicyMetadata newPolicy = SnapshotLifecyclePolicyMetadata.builder()
                .setPolicy(createPolicy("foo", "*/1 * * * * ?"))
                .setHeaders(Collections.emptyMap())
                .setVersion(2)
                .setModifiedDate(2)
                .build();
            Map<String, SnapshotLifecyclePolicyMetadata> policies = new HashMap<>();
            policies.put(newPolicy.getPolicy().getId(), newPolicy);
            ClusterState emptyState = createState(new SnapshotLifecycleMetadata(Collections.emptyMap(), OperationMode.RUNNING));
            ClusterState state = createState(new SnapshotLifecycleMetadata(policies, OperationMode.RUNNING));

            sls.clusterChanged(new ClusterChangedEvent("1", state, emptyState));

            // Since the service does not think it is master, it should not be triggered or scheduled
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.emptySet()));

            sls.onMaster();
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.singleton("initial-1")));

            state = createState(new SnapshotLifecycleMetadata(policies, OperationMode.STOPPING));
            sls.clusterChanged(new ClusterChangedEvent("2", state, emptyState));

            // Since the service is stopping, jobs should have been cancelled
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.emptySet()));

            state = createState(new SnapshotLifecycleMetadata(policies, OperationMode.STOPPED));
            sls.clusterChanged(new ClusterChangedEvent("3", state, emptyState));

            // Since the service is stopped, jobs should have been cancelled
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.emptySet()));

            threadPool.shutdownNow();
        }
    }

    /**
     * Test new policies getting scheduled correctly, updated policies also being scheduled,
     * and deleted policies having their schedules cancelled.
     */
    public void testPolicyCRUD() throws Exception {
        ClockMock clock = new ClockMock();
        final AtomicInteger triggerCount = new AtomicInteger(0);
        final AtomicReference<Consumer<SchedulerEngine.Event>> trigger = new AtomicReference<>(e -> triggerCount.incrementAndGet());
        try (ThreadPool threadPool = new TestThreadPool("test");
             ClusterService clusterService = ClusterServiceUtils.createClusterService(threadPool);
             SnapshotLifecycleService sls = new SnapshotLifecycleService(Settings.EMPTY,
                 () -> new FakeSnapshotTask(e -> trigger.get().accept(e)), clusterService, clock)) {

            sls.offMaster();
            SnapshotLifecycleMetadata snapMeta = new SnapshotLifecycleMetadata(Collections.emptyMap(), OperationMode.RUNNING);
            ClusterState previousState = createState(snapMeta);
            Map<String, SnapshotLifecyclePolicyMetadata> policies = new HashMap<>();

            SnapshotLifecyclePolicyMetadata policy = SnapshotLifecyclePolicyMetadata.builder()
                .setPolicy(createPolicy("foo", "*/1 * * * * ?"))
                .setHeaders(Collections.emptyMap())
                .setModifiedDate(1)
                .build();
            policies.put(policy.getPolicy().getId(), policy);
            snapMeta = new SnapshotLifecycleMetadata(policies, OperationMode.RUNNING);
            ClusterState state = createState(snapMeta);
            ClusterChangedEvent event = new ClusterChangedEvent("1", state, previousState);
            trigger.set(e -> {
                fail("trigger should not be invoked");
            });
            sls.clusterChanged(event);

            // Since the service does not think it is master, it should not be triggered or scheduled
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.emptySet()));

            // Change the service to think it's on the master node, events should be scheduled now
            sls.onMaster();
            trigger.set(e -> triggerCount.incrementAndGet());
            sls.clusterChanged(event);
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.singleton("foo-1")));

            assertBusy(() -> assertThat(triggerCount.get(), greaterThan(0)));

            clock.freeze();
            int currentCount = triggerCount.get();
            previousState = state;
            SnapshotLifecyclePolicyMetadata newPolicy = SnapshotLifecyclePolicyMetadata.builder()
                .setPolicy(createPolicy("foo", "*/1 * * * * ?"))
                .setHeaders(Collections.emptyMap())
                .setVersion(2)
                .setModifiedDate(2)
                .build();
            policies.put(policy.getPolicy().getId(), newPolicy);
            state = createState(new SnapshotLifecycleMetadata(policies, OperationMode.RUNNING));
            event = new ClusterChangedEvent("2", state, previousState);
            sls.clusterChanged(event);
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.singleton("foo-2")));

            trigger.set(e -> {
                // Make sure the job got updated
                assertThat(e.getJobName(), equalTo("foo-2"));
                triggerCount.incrementAndGet();
            });
            clock.fastForwardSeconds(1);

            assertBusy(() -> assertThat(triggerCount.get(), greaterThan(currentCount)));

            final int currentCount2 = triggerCount.get();
            previousState = state;
            // Create a state simulating the policy being deleted
            state = createState(new SnapshotLifecycleMetadata(Collections.emptyMap(), OperationMode.RUNNING));
            event = new ClusterChangedEvent("2", state, previousState);
            sls.clusterChanged(event);
            clock.fastForwardSeconds(2);

            // The existing job should be cancelled and no longer trigger
            assertThat(triggerCount.get(), equalTo(currentCount2));
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.emptySet()));

            // When the service is no longer master, all jobs should be automatically cancelled
            policy = SnapshotLifecyclePolicyMetadata.builder()
                .setPolicy(createPolicy("foo", "*/1 * * * * ?"))
                .setHeaders(Collections.emptyMap())
                .setVersion(3)
                .setModifiedDate(1)
                .build();
            policies.put(policy.getPolicy().getId(), policy);
            snapMeta = new SnapshotLifecycleMetadata(policies, OperationMode.RUNNING);
            previousState = state;
            state = createState(snapMeta);
            event = new ClusterChangedEvent("1", state, previousState);
            trigger.set(e -> triggerCount.incrementAndGet());
            sls.clusterChanged(event);
            clock.fastForwardSeconds(2);

            // Make sure at least one triggers and the job is scheduled
            assertBusy(() -> assertThat(triggerCount.get(), greaterThan(currentCount2)));
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.singleton("foo-3")));

            // Signify becoming non-master, the jobs should all be cancelled
            sls.offMaster();
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.emptySet()));

            threadPool.shutdownNow();
        }
    }

    /**
     * Test for policy ids ending in numbers the way generate job ids doesn't cause confusion
     */
    public void testPolicyNamesEndingInNumbers() throws Exception {
        ClockMock clock = new ClockMock();
        final AtomicInteger triggerCount = new AtomicInteger(0);
        final AtomicReference<Consumer<SchedulerEngine.Event>> trigger = new AtomicReference<>(e -> triggerCount.incrementAndGet());
        try (ThreadPool threadPool = new TestThreadPool("test");
             ClusterService clusterService = ClusterServiceUtils.createClusterService(threadPool);
             SnapshotLifecycleService sls = new SnapshotLifecycleService(Settings.EMPTY,
                 () -> new FakeSnapshotTask(e -> trigger.get().accept(e)), clusterService, clock)) {
            sls.onMaster();

            SnapshotLifecycleMetadata snapMeta = new SnapshotLifecycleMetadata(Collections.emptyMap(), OperationMode.RUNNING);
            ClusterState previousState = createState(snapMeta);
            Map<String, SnapshotLifecyclePolicyMetadata> policies = new HashMap<>();

            SnapshotLifecyclePolicyMetadata policy = SnapshotLifecyclePolicyMetadata.builder()
                .setPolicy(createPolicy("foo-2", "30 * * * * ?"))
                .setHeaders(Collections.emptyMap())
                .setVersion(1)
                .setModifiedDate(1)
                .build();
            policies.put(policy.getPolicy().getId(), policy);
            snapMeta = new SnapshotLifecycleMetadata(policies, OperationMode.RUNNING);
            ClusterState state = createState(snapMeta);
            ClusterChangedEvent event = new ClusterChangedEvent("1", state, previousState);
            sls.clusterChanged(event);

            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.singleton("foo-2-1")));

            previousState = state;
            SnapshotLifecyclePolicyMetadata secondPolicy = SnapshotLifecyclePolicyMetadata.builder()
                .setPolicy(createPolicy("foo-1", "45 * * * * ?"))
                .setHeaders(Collections.emptyMap())
                .setVersion(2)
                .setModifiedDate(1)
                .build();
            policies.put(secondPolicy.getPolicy().getId(), secondPolicy);
            snapMeta = new SnapshotLifecycleMetadata(policies, OperationMode.RUNNING);
            state = createState(snapMeta);
            event = new ClusterChangedEvent("2", state, previousState);
            sls.clusterChanged(event);

            assertThat(sls.getScheduler().scheduledJobIds(), containsInAnyOrder("foo-2-1", "foo-1-2"));

            sls.offMaster();
            assertThat(sls.getScheduler().scheduledJobIds(), equalTo(Collections.emptySet()));

            threadPool.shutdownNow();
        }
    }

    class FakeSnapshotTask extends SnapshotLifecycleTask {
        private final Consumer<SchedulerEngine.Event> onTriggered;

        FakeSnapshotTask(Consumer<SchedulerEngine.Event> onTriggered) {
            super(null, null, null);
            this.onTriggered = onTriggered;
        }

        @Override
        public void triggered(SchedulerEngine.Event event) {
            logger.info("--> fake snapshot task triggered");
            onTriggered.accept(event);
        }
    }

    public ClusterState createState(SnapshotLifecycleMetadata snapMeta) {
        MetaData metaData = MetaData.builder()
            .putCustom(SnapshotLifecycleMetadata.TYPE, snapMeta)
            .build();
        return ClusterState.builder(new ClusterName("cluster"))
            .metaData(metaData)
            .build();
    }

    public static SnapshotLifecyclePolicy createPolicy(String id) {
        return createPolicy(id, randomSchedule());
    }

    public static SnapshotLifecyclePolicy createPolicy(String id, String schedule) {
        Map<String, Object> config = new HashMap<>();
        config.put("ignore_unavailable", randomBoolean());
        List<String> indices = new ArrayList<>();
        indices.add("foo-*");
        indices.add(randomAlphaOfLength(4));
        config.put("indices", indices);
        return new SnapshotLifecyclePolicy(id, randomAlphaOfLength(4), schedule, randomAlphaOfLength(4), config);
    }

    private static String randomSchedule() {
        return randomIntBetween(0, 59) + " " +
            randomIntBetween(0, 59) + " " +
            randomIntBetween(0, 12) + " * * ?";
    }
}
