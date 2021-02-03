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
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequest;
import org.elasticsearch.action.admin.indices.alias.IndicesAliasesRequest.AliasActions;
import org.elasticsearch.action.support.master.AcknowledgedResponse;
import org.elasticsearch.cluster.metadata.AliasMetadata;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.xpack.core.ilm.AsyncActionStep.Listener;
import org.elasticsearch.xpack.core.ilm.Step.StepKey;
import org.mockito.Mockito;
import org.mockito.stubbing.Answer;

import java.util.Arrays;
import java.util.List;

import static org.elasticsearch.xpack.core.ilm.AbstractStepMasterTimeoutTestCase.emptyClusterState;
import static org.hamcrest.Matchers.equalTo;

public class ShrinkSetAliasStepTests extends AbstractStepTestCase<ShrinkSetAliasStep> {

    @Override
    public ShrinkSetAliasStep createRandomInstance() {
        StepKey stepKey = randomStepKey();
        StepKey nextStepKey = randomStepKey();
        String shrunkIndexPrefix = randomAlphaOfLength(10);
        return new ShrinkSetAliasStep(stepKey, nextStepKey, client, shrunkIndexPrefix);
    }

    @Override
    public ShrinkSetAliasStep mutateInstance(ShrinkSetAliasStep instance) {
        StepKey key = instance.getKey();
        StepKey nextKey = instance.getNextStepKey();
        String shrunkIndexPrefix = instance.getShrunkIndexPrefix();
        switch (between(0, 2)) {
        case 0:
            key = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        case 1:
            nextKey = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        case 2:
            shrunkIndexPrefix += randomAlphaOfLength(5);
            break;
        default:
            throw new AssertionError("Illegal randomisation branch");
        }
        return new ShrinkSetAliasStep(key, nextKey, instance.getClient(), shrunkIndexPrefix);
    }

    @Override
    public ShrinkSetAliasStep copyInstance(ShrinkSetAliasStep instance) {
        return new ShrinkSetAliasStep(instance.getKey(), instance.getNextStepKey(), instance.getClient(), instance.getShrunkIndexPrefix());
    }

    public void testPerformAction() {
        IndexMetadata.Builder indexMetadataBuilder = IndexMetadata.builder(randomAlphaOfLength(10)).settings(settings(Version.CURRENT))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5));
        AliasMetadata.Builder aliasBuilder = AliasMetadata.builder(randomAlphaOfLengthBetween(3, 10));
        if (randomBoolean()) {
            aliasBuilder.routing(randomAlphaOfLengthBetween(3, 10));
        }
        if (randomBoolean()) {
            aliasBuilder.searchRouting(randomAlphaOfLengthBetween(3, 10));
        }
        if (randomBoolean()) {
            aliasBuilder.indexRouting(randomAlphaOfLengthBetween(3, 10));
        }
        String aliasMetadataFilter = randomBoolean() ? null : "{\"term\":{\"year\":2016}}";
        aliasBuilder.filter(aliasMetadataFilter);
        aliasBuilder.writeIndex(randomBoolean());
        AliasMetadata aliasMetadata = aliasBuilder.build();
        IndexMetadata indexMetadata = indexMetadataBuilder.putAlias(aliasMetadata).build();
        ShrinkSetAliasStep step = createRandomInstance();

        String sourceIndex = indexMetadata.getIndex().getName();
        String shrunkenIndex = step.getShrunkIndexPrefix() + sourceIndex;
        List<AliasActions> expectedAliasActions = Arrays.asList(
            IndicesAliasesRequest.AliasActions.removeIndex().index(sourceIndex),
            IndicesAliasesRequest.AliasActions.add().index(shrunkenIndex).alias(sourceIndex),
            IndicesAliasesRequest.AliasActions.add().index(shrunkenIndex).alias(aliasMetadata.alias())
                .searchRouting(aliasMetadata.searchRouting()).indexRouting(aliasMetadata.indexRouting())
                .filter(aliasMetadataFilter).writeIndex(null));

        Mockito.doAnswer( invocation -> {
            IndicesAliasesRequest request = (IndicesAliasesRequest) invocation.getArguments()[0];
            assertThat(request.getAliasActions(), equalTo(expectedAliasActions));
            @SuppressWarnings("unchecked")
            ActionListener<AcknowledgedResponse> listener = (ActionListener<AcknowledgedResponse>) invocation.getArguments()[1];
            listener.onResponse(AcknowledgedResponse.TRUE);
            return null;
        }).when(indicesClient).aliases(Mockito.any(), Mockito.any());

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

        assertTrue(actionCompleted.get());

        Mockito.verify(client, Mockito.only()).admin();
        Mockito.verify(adminClient, Mockito.only()).indices();
        Mockito.verify(indicesClient, Mockito.only()).aliases(Mockito.any(), Mockito.any());
    }

    public void testPerformActionFailure() {
        IndexMetadata indexMetadata = IndexMetadata.builder(randomAlphaOfLength(10)).settings(settings(Version.CURRENT))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        Exception exception = new RuntimeException();
        ShrinkSetAliasStep step = createRandomInstance();

        Mockito.doAnswer((Answer<Void>) invocation -> {
            @SuppressWarnings("unchecked")
            ActionListener<AcknowledgedResponse> listener = (ActionListener<AcknowledgedResponse>) invocation.getArguments()[1];
            listener.onFailure(exception);
            return null;
        }).when(indicesClient).aliases(Mockito.any(), Mockito.any());

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
        Mockito.verify(indicesClient, Mockito.only()).aliases(Mockito.any(), Mockito.any());
    }

}
