/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.indexlifecycle;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.Version;
import org.elasticsearch.cluster.ClusterName;
import org.elasticsearch.cluster.ClusterState;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.cluster.metadata.MetaData;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.Index;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.indexlifecycle.LifecyclePolicy;
import org.elasticsearch.xpack.core.indexlifecycle.LifecycleSettings;
import org.elasticsearch.xpack.core.indexlifecycle.MockStep;
import org.elasticsearch.xpack.core.indexlifecycle.Phase;
import org.elasticsearch.xpack.core.indexlifecycle.Step;
import org.elasticsearch.xpack.core.indexlifecycle.Step.StepKey;
import org.elasticsearch.xpack.core.indexlifecycle.TestLifecycleType;
import org.junit.Before;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.sameInstance;
import static org.mockito.Mockito.mock;

public class MoveToNextStepUpdateTaskTests extends ESTestCase {

    String policy;
    ClusterState clusterState;
    Index index;
    PolicyStepsRegistry stepsRegistry;

    @Before
    public void setupClusterState() {
        policy = randomAlphaOfLength(10);
        IndexMetaData indexMetadata = IndexMetaData.builder(randomAlphaOfLength(5))
            .settings(settings(Version.CURRENT)
                .put(LifecycleSettings.LIFECYCLE_NAME, policy))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        index = indexMetadata.getIndex();
        MetaData metaData = MetaData.builder()
            .persistentSettings(settings(Version.CURRENT).build())
            .put(IndexMetaData.builder(indexMetadata))
            .build();
        clusterState = ClusterState.builder(ClusterName.DEFAULT).metaData(metaData).build();


        Step currentStep = new MockStep(new StepKey("current-phase", "current-action", "current-name"), null);
        Step nextStep = new MockStep(new StepKey("next-phase", "next-action", "next-name"), null);
        Map<StepKey, Step> stepMap = new HashMap<>();
        stepMap.put(currentStep.getKey(), currentStep);
        stepMap.put(nextStep.getKey(), nextStep);
        Map<String, Map<Step.StepKey, Step>> policyMap = Collections.singletonMap(policy, stepMap);
        stepsRegistry = new PolicyStepsRegistry(null, null, policyMap);
    }

    public void testExecuteSuccessfullyMoved() {
        StepKey currentStepKey = new StepKey("current-phase", "current-action", "current-name");
        StepKey nextStepKey = new StepKey("next-phase", "next-action", "next-name");
        long now = randomNonNegativeLong();

        setStateToKey(currentStepKey);

        SetOnce<Boolean> changed = new SetOnce<>();
        MoveToNextStepUpdateTask.Listener listener = (c) -> changed.set(true);
        MoveToNextStepUpdateTask task = new MoveToNextStepUpdateTask(index, policy, currentStepKey, nextStepKey, () -> now,
            stepsRegistry, listener);
        ClusterState newState = task.execute(clusterState);
        StepKey actualKey = IndexLifecycleRunner.getCurrentStepKey(newState.metaData().index(index).getSettings());
        assertThat(actualKey, equalTo(nextStepKey));
        assertThat(LifecycleSettings.LIFECYCLE_PHASE_TIME_SETTING.get(newState.metaData().index(index).getSettings()), equalTo(now));
        assertThat(LifecycleSettings.LIFECYCLE_ACTION_TIME_SETTING.get(newState.metaData().index(index).getSettings()), equalTo(now));
        assertThat(LifecycleSettings.LIFECYCLE_STEP_TIME_SETTING.get(newState.metaData().index(index).getSettings()), equalTo(now));
        task.clusterStateProcessed("source", clusterState, newState);
        assertTrue(changed.get());
    }

    public void testExecuteDifferentCurrentStep() {
        StepKey currentStepKey = new StepKey("current-phase", "current-action", "current-name");
        StepKey notCurrentStepKey = new StepKey("not-current", "not-current", "not-current");
        long now = randomNonNegativeLong();
        setStateToKey(notCurrentStepKey);
        MoveToNextStepUpdateTask.Listener listener = (c) -> {
        };
        MoveToNextStepUpdateTask task = new MoveToNextStepUpdateTask(index, policy, currentStepKey, null, () -> now,
            stepsRegistry, listener);
        ClusterState newState = task.execute(clusterState);
        assertSame(newState, clusterState);
    }

    public void testExecuteDifferentPolicy() {
        StepKey currentStepKey = new StepKey("current-phase", "current-action", "current-name");
        long now = randomNonNegativeLong();
        setStateToKey(currentStepKey);
        setStatePolicy("not-" + policy);
        MoveToNextStepUpdateTask.Listener listener = (c) -> {};
        MoveToNextStepUpdateTask task = new MoveToNextStepUpdateTask(index, policy, currentStepKey, null, () -> now,
            stepsRegistry, listener);
        ClusterState newState = task.execute(clusterState);
        assertSame(newState, clusterState);
    }

    public void testExecuteSuccessfulMoveWithInvalidNextStep() {
        StepKey currentStepKey = new StepKey("current-phase", "current-action", "current-name");
        StepKey invalidNextStep = new StepKey("next-invalid", "next-invalid", "next-invalid");
        long now = randomNonNegativeLong();

        setStateToKey(currentStepKey);

        SetOnce<Boolean> changed = new SetOnce<>();
        MoveToNextStepUpdateTask.Listener listener = (c) -> changed.set(true);
        MoveToNextStepUpdateTask task = new MoveToNextStepUpdateTask(index, policy, currentStepKey, invalidNextStep, () -> now,
            stepsRegistry, listener);
        ClusterState newState = task.execute(clusterState);
        StepKey actualKey = IndexLifecycleRunner.getCurrentStepKey(newState.metaData().index(index).getSettings());
        assertThat(actualKey, equalTo(invalidNextStep));
        assertThat(LifecycleSettings.LIFECYCLE_PHASE_TIME_SETTING.get(newState.metaData().index(index).getSettings()), equalTo(now));
        assertThat(LifecycleSettings.LIFECYCLE_ACTION_TIME_SETTING.get(newState.metaData().index(index).getSettings()), equalTo(now));
        assertThat(LifecycleSettings.LIFECYCLE_STEP_TIME_SETTING.get(newState.metaData().index(index).getSettings()), equalTo(now));
        task.clusterStateProcessed("source", clusterState, newState);
        assertTrue(changed.get());
    }

    public void testClusterProcessedWithNoChange() {
        StepKey currentStepKey = new StepKey("current-phase", "current-action", "current-name");
        long now = randomNonNegativeLong();
        setStateToKey(currentStepKey);
        SetOnce<Boolean> changed = new SetOnce<>();
        MoveToNextStepUpdateTask.Listener listener = (c) -> changed.set(true);
        MoveToNextStepUpdateTask task = new MoveToNextStepUpdateTask(index, policy, currentStepKey, null, () -> now,
            stepsRegistry, listener);
        task.clusterStateProcessed("source", clusterState, clusterState);
        assertNull(changed.get());
    }

    public void testOnFailure() {
        StepKey currentStepKey = new StepKey("current-phase", "current-action", "current-name");
        StepKey nextStepKey = new StepKey("next-phase", "next-action", "next-name");
        long now = randomNonNegativeLong();

        setStateToKey(currentStepKey);

        SetOnce<Boolean> changed = new SetOnce<>();
        MoveToNextStepUpdateTask.Listener listener = (c) -> changed.set(true);
        MoveToNextStepUpdateTask task = new MoveToNextStepUpdateTask(index, policy, currentStepKey, nextStepKey, () -> now,
            stepsRegistry, listener);
        Exception expectedException = new RuntimeException();
        ElasticsearchException exception = expectThrows(ElasticsearchException.class,
                () -> task.onFailure(randomAlphaOfLength(10), expectedException));
        assertEquals("policy [" + policy + "] for index [" + index.getName() + "] failed trying to move from step [" + currentStepKey
                + "] to step [" + nextStepKey + "].", exception.getMessage());
        assertSame(expectedException, exception.getCause());
    }

    private void setStatePolicy(String policy) {
        clusterState = ClusterState.builder(clusterState)
            .metaData(MetaData.builder(clusterState.metaData())
                .updateSettings(Settings.builder()
                    .put(LifecycleSettings.LIFECYCLE_NAME, policy).build(), index.getName())).build();

    }
    private void setStateToKey(StepKey stepKey) {
        clusterState = ClusterState.builder(clusterState)
            .metaData(MetaData.builder(clusterState.metaData())
                .updateSettings(Settings.builder()
                    .put(LifecycleSettings.LIFECYCLE_PHASE, stepKey.getPhase())
                    .put(LifecycleSettings.LIFECYCLE_ACTION, stepKey.getAction())
                    .put(LifecycleSettings.LIFECYCLE_STEP, stepKey.getName()).build(), index.getName())).build();
    }
}
