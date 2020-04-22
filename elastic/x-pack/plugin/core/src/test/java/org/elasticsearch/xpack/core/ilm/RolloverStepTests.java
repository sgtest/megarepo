/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ilm;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.rollover.MaxSizeCondition;
import org.elasticsearch.action.admin.indices.rollover.RolloverInfo;
import org.elasticsearch.action.admin.indices.rollover.RolloverRequest;
import org.elasticsearch.action.admin.indices.rollover.RolloverResponse;
import org.elasticsearch.cluster.metadata.AliasMetadata;
import org.elasticsearch.cluster.metadata.IndexMetadata;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.xpack.core.ilm.Step.StepKey;
import org.mockito.Mockito;

import java.util.Collections;
import java.util.Locale;

import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.core.Is.is;

public class RolloverStepTests extends AbstractStepMasterTimeoutTestCase<RolloverStep> {

    @Override
    public RolloverStep createRandomInstance() {
        StepKey stepKey = randomStepKey();
        StepKey nextStepKey = randomStepKey();

        return new RolloverStep(stepKey, nextStepKey, client);
    }

    @Override
    public RolloverStep mutateInstance(RolloverStep instance) {
        StepKey key = instance.getKey();
        StepKey nextKey = instance.getNextStepKey();


        switch (between(0, 1)) {
        case 0:
            key = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        case 1:
            nextKey = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        default:
            throw new AssertionError("Illegal randomisation branch");
        }

        return new RolloverStep(key, nextKey, instance.getClient());
    }

    @Override
    public RolloverStep copyInstance(RolloverStep instance) {
        return new RolloverStep(instance.getKey(), instance.getNextStepKey(), instance.getClient());
    }

    private IndexMetadata getIndexMetadata(String alias) {
        return IndexMetadata.builder(randomAlphaOfLength(10))
            .putAlias(AliasMetadata.builder(alias))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
    }

    @Override
    protected IndexMetadata getIndexMetadata() {
        return getIndexMetadata(randomAlphaOfLength(5));
    }

    private static void assertRolloverIndexRequest(RolloverRequest request, String alias) {
        assertNotNull(request);
        assertEquals(1, request.indices().length);
        assertEquals(alias, request.indices()[0]);
        assertEquals(alias, request.getRolloverTarget());
        assertFalse(request.isDryRun());
        assertEquals(0, request.getConditions().size());
    }

    public void testPerformAction() {
        String alias = randomAlphaOfLength(5);
        IndexMetadata indexMetadata = getIndexMetadata(alias);

        RolloverStep step = createRandomInstance();

        Mockito.doAnswer(invocation -> {
            RolloverRequest request = (RolloverRequest) invocation.getArguments()[0];
            @SuppressWarnings("unchecked")
            ActionListener<RolloverResponse> listener = (ActionListener<RolloverResponse>) invocation.getArguments()[1];
            assertRolloverIndexRequest(request, alias);
            listener.onResponse(new RolloverResponse(null, null, Collections.emptyMap(), request.isDryRun(), true, true, true));
            return null;
        }).when(indicesClient).rolloverIndex(Mockito.any(), Mockito.any());

        SetOnce<Boolean> actionCompleted = new SetOnce<>();
        step.performAction(indexMetadata, emptyClusterState(), null, new AsyncActionStep.Listener() {

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
        Mockito.verify(indicesClient, Mockito.only()).rolloverIndex(Mockito.any(), Mockito.any());
    }

    public void testPerformActionWithIndexingComplete() {
        String alias = randomAlphaOfLength(5);
        IndexMetadata indexMetadata = IndexMetadata.builder(randomAlphaOfLength(10))
            .putAlias(AliasMetadata.builder(alias))
            .settings(settings(Version.CURRENT)
                .put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias)
                .put(LifecycleSettings.LIFECYCLE_INDEXING_COMPLETE, true))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();

        RolloverStep step = createRandomInstance();

        SetOnce<Boolean> actionCompleted = new SetOnce<>();
        step.performAction(indexMetadata, null, null, new AsyncActionStep.Listener() {

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
    }

    public void testPerformActionSkipsRolloverForAlreadyRolledIndex() {
        String rolloverAlias = randomAlphaOfLength(5);
        IndexMetadata indexMetadata = IndexMetadata.builder(randomAlphaOfLength(10))
            .putAlias(AliasMetadata.builder(rolloverAlias))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, rolloverAlias))
            .putRolloverInfo(new RolloverInfo(rolloverAlias,
                Collections.singletonList(new MaxSizeCondition(new ByteSizeValue(2L))),
                System.currentTimeMillis())
            )
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();

        RolloverStep step = createRandomInstance();

        step.performAction(indexMetadata, null, null, new AsyncActionStep.Listener() {

            @Override
            public void onResponse(boolean complete) {
                assertThat(complete, is(true));
            }

            @Override
            public void onFailure(Exception e) {
                throw new AssertionError("Unexpected method call", e);
            }
        });

        Mockito.verify(indicesClient, Mockito.never()).rolloverIndex(Mockito.any(), Mockito.any());
    }

    public void testPerformActionFailure() {
        String alias = randomAlphaOfLength(5);
        IndexMetadata indexMetadata = getIndexMetadata(alias);
        Exception exception = new RuntimeException();
        RolloverStep step = createRandomInstance();

        Mockito.doAnswer(invocation -> {
            RolloverRequest request = (RolloverRequest) invocation.getArguments()[0];
            @SuppressWarnings("unchecked")
            ActionListener<RolloverResponse> listener = (ActionListener<RolloverResponse>) invocation.getArguments()[1];
            assertRolloverIndexRequest(request, alias);
            listener.onFailure(exception);
            return null;
        }).when(indicesClient).rolloverIndex(Mockito.any(), Mockito.any());

        SetOnce<Boolean> exceptionThrown = new SetOnce<>();
        step.performAction(indexMetadata, emptyClusterState(), null, new AsyncActionStep.Listener() {

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
        Mockito.verify(indicesClient, Mockito.only()).rolloverIndex(Mockito.any(), Mockito.any());
    }

    public void testPerformActionInvalidNullOrEmptyAlias() {
        String alias = randomBoolean() ? "" : null;
        IndexMetadata indexMetadata = IndexMetadata.builder(randomAlphaOfLength(10))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        RolloverStep step = createRandomInstance();

        SetOnce<Exception> exceptionThrown = new SetOnce<>();
        step.performAction(indexMetadata, null, null, new AsyncActionStep.Listener() {
            @Override
            public void onResponse(boolean complete) {
                throw new AssertionError("Unexpected method call");
            }

            @Override
            public void onFailure(Exception e) {
                exceptionThrown.set(e);
            }
        });
        assertThat(exceptionThrown.get().getClass(), equalTo(IllegalArgumentException.class));
        assertThat(exceptionThrown.get().getMessage(), equalTo(String.format(Locale.ROOT,
            "setting [%s] for index [%s] is empty or not defined", RolloverAction.LIFECYCLE_ROLLOVER_ALIAS,
            indexMetadata.getIndex().getName())));
    }

    public void testPerformActionAliasDoesNotPointToIndex() {
        String alias = randomAlphaOfLength(5);
        IndexMetadata indexMetadata = IndexMetadata.builder(randomAlphaOfLength(10))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        RolloverStep step = createRandomInstance();

        SetOnce<Exception> exceptionThrown = new SetOnce<>();
        step.performAction(indexMetadata, null, null, new AsyncActionStep.Listener() {
            @Override
            public void onResponse(boolean complete) {
                throw new AssertionError("Unexpected method call");
            }

            @Override
            public void onFailure(Exception e) {
                exceptionThrown.set(e);
            }
        });
        assertThat(exceptionThrown.get().getClass(), equalTo(IllegalArgumentException.class));
        assertThat(exceptionThrown.get().getMessage(), equalTo(String.format(Locale.ROOT,
            "%s [%s] does not point to index [%s]", RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias,
            indexMetadata.getIndex().getName())));
    }
}
