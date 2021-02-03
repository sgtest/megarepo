/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ilm;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.rollover.RolloverResponse;
import org.elasticsearch.action.admin.indices.shrink.ResizeRequest;
import org.elasticsearch.action.admin.indices.shrink.ResizeResponse;
import org.elasticsearch.cluster.metadata.AliasMetadata;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.xpack.core.ilm.AsyncActionStep.Listener;
import org.elasticsearch.xpack.core.ilm.Step.StepKey;
import org.mockito.Mockito;

import java.util.Collections;

import static org.elasticsearch.xpack.core.ilm.AbstractStepMasterTimeoutTestCase.emptyClusterState;
import static org.elasticsearch.xpack.core.ilm.LifecycleExecutionState.ILM_CUSTOM_METADATA_KEY;
import static org.hamcrest.Matchers.equalTo;

public class ShrinkStepTests extends AbstractStepTestCase<ShrinkStep> {

    @Override
    public ShrinkStep createRandomInstance() {
        StepKey stepKey = randomStepKey();
        StepKey nextStepKey = randomStepKey();
        Integer numberOfShards = null;
        ByteSizeValue maxSinglePrimarySize = null;
        if (randomBoolean()) {
            numberOfShards = randomIntBetween(1, 20);
        } else {
            maxSinglePrimarySize = new ByteSizeValue(between(1,100));
        }
        String shrunkIndexPrefix = randomAlphaOfLength(10);
        return new ShrinkStep(stepKey, nextStepKey, client, numberOfShards, maxSinglePrimarySize, shrunkIndexPrefix);
    }

    @Override
    public ShrinkStep mutateInstance(ShrinkStep instance) {
        StepKey key = instance.getKey();
        StepKey nextKey = instance.getNextStepKey();
        Integer numberOfShards = instance.getNumberOfShards();
        ByteSizeValue maxSinglePrimarySize = instance.getMaxSinglePrimarySize();
        String shrunkIndexPrefix = instance.getShrunkIndexPrefix();

        switch (between(0, 3)) {
        case 0:
            key = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        case 1:
            nextKey = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        case 2:
            if (numberOfShards != null) {
                numberOfShards = numberOfShards + 1;
            }
            if (maxSinglePrimarySize != null) {
                maxSinglePrimarySize = new ByteSizeValue(maxSinglePrimarySize.getBytes() + 1);
            }
            break;
        case 3:
            shrunkIndexPrefix += randomAlphaOfLength(5);
            break;
        default:
            throw new AssertionError("Illegal randomisation branch");
        }

        return new ShrinkStep(key, nextKey, instance.getClient(), numberOfShards, maxSinglePrimarySize, shrunkIndexPrefix);
    }

    @Override
    public ShrinkStep copyInstance(ShrinkStep instance) {
        return new ShrinkStep(instance.getKey(), instance.getNextStepKey(), instance.getClient(), instance.getNumberOfShards(),
            instance.getMaxSinglePrimarySize(), instance.getShrunkIndexPrefix());
    }

    public void testPerformAction() throws Exception {
        String lifecycleName = randomAlphaOfLength(5);
        ShrinkStep step = createRandomInstance();
        LifecycleExecutionState.Builder lifecycleState = LifecycleExecutionState.builder();
        lifecycleState.setPhase(step.getKey().getPhase());
        lifecycleState.setAction(step.getKey().getAction());
        lifecycleState.setStep(step.getKey().getName());
        lifecycleState.setIndexCreationDate(randomNonNegativeLong());
        IndexMetadata sourceIndexMetadata = IndexMetadata.builder(randomAlphaOfLength(10))
            .settings(settings(Version.CURRENT)
                .put(LifecycleSettings.LIFECYCLE_NAME, lifecycleName)
            )
            .putCustom(ILM_CUSTOM_METADATA_KEY, lifecycleState.build().asMap())
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5))
            .putAlias(AliasMetadata.builder("my_alias"))
            .build();

        Mockito.doAnswer(invocation -> {
            ResizeRequest request = (ResizeRequest) invocation.getArguments()[0];
            @SuppressWarnings("unchecked")
            ActionListener<ResizeResponse> listener = (ActionListener<ResizeResponse>) invocation.getArguments()[1];
            assertThat(request.getSourceIndex(), equalTo(sourceIndexMetadata.getIndex().getName()));
            assertThat(request.getTargetIndexRequest().aliases(), equalTo(Collections.emptySet()));

            Settings.Builder builder = Settings.builder();
            builder.put(IndexMetadata.SETTING_NUMBER_OF_REPLICAS, sourceIndexMetadata.getNumberOfReplicas())
                .put(LifecycleSettings.LIFECYCLE_NAME, lifecycleName)
                .put(IndexMetadata.INDEX_ROUTING_REQUIRE_GROUP_SETTING.getKey() + "_id", (String) null);
            if (step.getNumberOfShards() != null) {
                builder.put(IndexMetadata.SETTING_NUMBER_OF_SHARDS, step.getNumberOfShards());
            }
            assertThat(request.getTargetIndexRequest().settings(), equalTo(builder.build()));
            if (step.getNumberOfShards() != null) {
                assertThat(request.getTargetIndexRequest().settings()
                    .getAsInt(IndexMetadata.SETTING_NUMBER_OF_SHARDS, -1), equalTo(step.getNumberOfShards()));
            }
            request.setMaxSinglePrimarySize(step.getMaxSinglePrimarySize());
            listener.onResponse(new ResizeResponse(true, true, sourceIndexMetadata.getIndex().getName()));
            return null;
        }).when(indicesClient).resizeIndex(Mockito.any(), Mockito.any());

        SetOnce<Boolean> actionCompleted = new SetOnce<>();
        step.performAction(sourceIndexMetadata, emptyClusterState(), null, new Listener() {

            @Override
            public void onResponse(boolean complete) {
                actionCompleted.set(complete);
            }

            @Override
            public void onFailure(Exception e) {
                throw new AssertionError("Unexpected method call", e);
            }
        });

        assertEquals(true, actionCompleted.get());

        Mockito.verify(client, Mockito.only()).admin();
        Mockito.verify(adminClient, Mockito.only()).indices();
        Mockito.verify(indicesClient, Mockito.only()).resizeIndex(Mockito.any(), Mockito.any());
    }

    public void testPerformActionIsCompleteForUnAckedRequests() throws Exception {
        LifecycleExecutionState.Builder lifecycleState = LifecycleExecutionState.builder();
        lifecycleState.setIndexCreationDate(randomNonNegativeLong());
        IndexMetadata indexMetadata = IndexMetadata.builder(randomAlphaOfLength(10)).settings(settings(Version.CURRENT))
            .putCustom(ILM_CUSTOM_METADATA_KEY, lifecycleState.build().asMap())
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        ShrinkStep step = createRandomInstance();

        Mockito.doAnswer(invocation -> {
            @SuppressWarnings("unchecked")
            ActionListener<ResizeResponse> listener = (ActionListener<ResizeResponse>) invocation.getArguments()[1];
            listener.onResponse(new ResizeResponse(false, false, indexMetadata.getIndex().getName()));
            return null;
        }).when(indicesClient).resizeIndex(Mockito.any(), Mockito.any());

        SetOnce<Boolean> actionCompleted = new SetOnce<>();
        step.performAction(indexMetadata, emptyClusterState(), null, new Listener() {

            @Override
            public void onResponse(boolean complete) {
                actionCompleted.set(complete);
            }

            @Override
            public void onFailure(Exception e) {
                throw new AssertionError("Unexpected method call", e);
            }
        });

        assertEquals(true, actionCompleted.get());

        Mockito.verify(client, Mockito.only()).admin();
        Mockito.verify(adminClient, Mockito.only()).indices();
        Mockito.verify(indicesClient, Mockito.only()).resizeIndex(Mockito.any(), Mockito.any());
    }

    public void testPerformActionFailure() throws Exception {
        LifecycleExecutionState.Builder lifecycleState = LifecycleExecutionState.builder();
        lifecycleState.setIndexCreationDate(randomNonNegativeLong());
        IndexMetadata indexMetadata = IndexMetadata.builder(randomAlphaOfLength(10)).settings(settings(Version.CURRENT))
            .putCustom(ILM_CUSTOM_METADATA_KEY, lifecycleState.build().asMap())
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        Exception exception = new RuntimeException();
        ShrinkStep step = createRandomInstance();

        Mockito.doAnswer(invocation -> {
            @SuppressWarnings("unchecked")
            ActionListener<RolloverResponse> listener = (ActionListener<RolloverResponse>) invocation.getArguments()[1];
            listener.onFailure(exception);
            return null;
        }).when(indicesClient).resizeIndex(Mockito.any(), Mockito.any());

        SetOnce<Boolean> exceptionThrown = new SetOnce<>();
        step.performAction(indexMetadata, emptyClusterState(), null, new Listener() {

            @Override
            public void onResponse(boolean complete) {
                throw new AssertionError("Unexpected method call");
            }

            @Override
            public void onFailure(Exception e) {
                assertSame(exception, e);
                exceptionThrown.set(true);
            }
        });

        assertEquals(true, exceptionThrown.get());

        Mockito.verify(client, Mockito.only()).admin();
        Mockito.verify(adminClient, Mockito.only()).indices();
        Mockito.verify(indicesClient, Mockito.only()).resizeIndex(Mockito.any(), Mockito.any());
    }

}
