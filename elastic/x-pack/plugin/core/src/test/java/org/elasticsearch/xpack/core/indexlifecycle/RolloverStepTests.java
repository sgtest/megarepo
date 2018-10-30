/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.indexlifecycle;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.Version;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.admin.indices.rollover.Condition;
import org.elasticsearch.action.admin.indices.rollover.MaxAgeCondition;
import org.elasticsearch.action.admin.indices.rollover.MaxDocsCondition;
import org.elasticsearch.action.admin.indices.rollover.MaxSizeCondition;
import org.elasticsearch.action.admin.indices.rollover.RolloverRequest;
import org.elasticsearch.action.admin.indices.rollover.RolloverResponse;
import org.elasticsearch.client.AdminClient;
import org.elasticsearch.client.Client;
import org.elasticsearch.client.IndicesAdminClient;
import org.elasticsearch.cluster.metadata.AliasMetaData;
import org.elasticsearch.cluster.metadata.IndexMetaData;
import org.elasticsearch.common.unit.ByteSizeUnit;
import org.elasticsearch.common.unit.ByteSizeValue;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.xpack.core.indexlifecycle.Step.StepKey;
import org.junit.Before;
import org.mockito.Mockito;
import org.mockito.invocation.InvocationOnMock;
import org.mockito.stubbing.Answer;

import java.util.Collections;
import java.util.HashSet;
import java.util.Locale;
import java.util.Set;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.equalTo;

public class RolloverStepTests extends AbstractStepTestCase<RolloverStep> {

    private Client client;

    @Before
    public void setup() {
        client = Mockito.mock(Client.class);
    }

    @Override
    public RolloverStep createRandomInstance() {
        StepKey stepKey = randomStepKey();
        StepKey nextStepKey = randomStepKey();
        ByteSizeUnit maxSizeUnit = randomFrom(ByteSizeUnit.values());
        ByteSizeValue maxSize = randomBoolean() ? null : new ByteSizeValue(randomNonNegativeLong() / maxSizeUnit.toBytes(1), maxSizeUnit);
        Long maxDocs = randomBoolean() ? null : randomNonNegativeLong();
        TimeValue maxAge = (maxDocs == null && maxSize == null || randomBoolean())
                ? TimeValue.parseTimeValue(randomPositiveTimeValue(), "rollover_action_test")
                : null;
        return new RolloverStep(stepKey, nextStepKey, client, maxSize, maxAge, maxDocs);
    }

    @Override
    public RolloverStep mutateInstance(RolloverStep instance) {
        StepKey key = instance.getKey();
        StepKey nextKey = instance.getNextStepKey();
        ByteSizeValue maxSize = instance.getMaxSize();
        TimeValue maxAge = instance.getMaxAge();
        Long maxDocs = instance.getMaxDocs();

        switch (between(0, 4)) {
        case 0:
            key = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        case 1:
            nextKey = new StepKey(key.getPhase(), key.getAction(), key.getName() + randomAlphaOfLength(5));
            break;
        case 2:
            maxSize = randomValueOtherThan(maxSize, () -> {
                ByteSizeUnit maxSizeUnit = randomFrom(ByteSizeUnit.values());
                return new ByteSizeValue(randomNonNegativeLong() / maxSizeUnit.toBytes(1), maxSizeUnit);
            });
            break;
        case 3:
            maxAge = TimeValue.parseTimeValue(randomPositiveTimeValue(), "rollover_action_test");
            break;
        case 4:
            maxDocs = randomNonNegativeLong();
            break;
        default:
            throw new AssertionError("Illegal randomisation branch");
        }

        return new RolloverStep(key, nextKey, instance.getClient(), maxSize, maxAge, maxDocs);
    }

    @Override
    public RolloverStep copyInstance(RolloverStep instance) {
        return new RolloverStep(instance.getKey(), instance.getNextStepKey(), instance.getClient(),
            instance.getMaxSize(), instance.getMaxAge(), instance.getMaxDocs());
    }

    private static void assertRolloverIndexRequest(RolloverRequest request, String alias, Set<Condition<?>> expectedConditions) {
        assertNotNull(request);
        assertEquals(1, request.indices().length);
        assertEquals(alias, request.indices()[0]);
        assertEquals(alias, request.getAlias());
        assertEquals(expectedConditions.size(), request.getConditions().size());
        Set<Object> expectedConditionValues = expectedConditions.stream().map(Condition::value).collect(Collectors.toSet());
        Set<Object> actualConditionValues = request.getConditions().values().stream()
            .map(Condition::value).collect(Collectors.toSet());
        assertEquals(expectedConditionValues, actualConditionValues);
    }

    public void testPerformAction() {
        String alias = randomAlphaOfLength(5);
        IndexMetaData indexMetaData = IndexMetaData.builder(randomAlphaOfLength(10))
            .putAlias(AliasMetaData.builder(alias))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();

        RolloverStep step = createRandomInstance();

        AdminClient adminClient = Mockito.mock(AdminClient.class);
        IndicesAdminClient indicesClient = Mockito.mock(IndicesAdminClient.class);

        Mockito.when(client.admin()).thenReturn(adminClient);
        Mockito.when(adminClient.indices()).thenReturn(indicesClient);
        Mockito.doAnswer(new Answer<Void>() {

            @Override
            public Void answer(InvocationOnMock invocation) throws Throwable {
                RolloverRequest request = (RolloverRequest) invocation.getArguments()[0];
                @SuppressWarnings("unchecked")
                ActionListener<RolloverResponse> listener = (ActionListener<RolloverResponse>) invocation.getArguments()[1];
                Set<Condition<?>> expectedConditions = new HashSet<>();
                if (step.getMaxAge() != null) {
                    expectedConditions.add(new MaxAgeCondition(step.getMaxAge()));
                }
                if (step.getMaxSize() != null) {
                    expectedConditions.add(new MaxSizeCondition(step.getMaxSize()));
                }
                if (step.getMaxDocs() != null) {
                    expectedConditions.add(new MaxDocsCondition(step.getMaxDocs()));
                }
                assertRolloverIndexRequest(request, alias, expectedConditions);
                listener.onResponse(new RolloverResponse(null, null, Collections.emptyMap(), request.isDryRun(), true, true, true));
                return null;
            }

        }).when(indicesClient).rolloverIndex(Mockito.any(), Mockito.any());

        SetOnce<Boolean> actionCompleted = new SetOnce<>();
        step.evaluateCondition(indexMetaData, new AsyncWaitStep.Listener() {

            @Override
            public void onResponse(boolean complete, ToXContentObject obj) {
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

    public void testPerformActionNotComplete() {
        String alias = randomAlphaOfLength(5);
        IndexMetaData indexMetaData = IndexMetaData.builder(randomAlphaOfLength(10))
            .putAlias(AliasMetaData.builder(alias))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        RolloverStep step = createRandomInstance();

        AdminClient adminClient = Mockito.mock(AdminClient.class);
        IndicesAdminClient indicesClient = Mockito.mock(IndicesAdminClient.class);

        Mockito.when(client.admin()).thenReturn(adminClient);
        Mockito.when(adminClient.indices()).thenReturn(indicesClient);
        Mockito.doAnswer(new Answer<Void>() {

            @Override
            public Void answer(InvocationOnMock invocation) throws Throwable {
                RolloverRequest request = (RolloverRequest) invocation.getArguments()[0];
                @SuppressWarnings("unchecked")
                ActionListener<RolloverResponse> listener = (ActionListener<RolloverResponse>) invocation.getArguments()[1];
                Set<Condition<?>> expectedConditions = new HashSet<>();
                if (step.getMaxAge() != null) {
                    expectedConditions.add(new MaxAgeCondition(step.getMaxAge()));
                }
                if (step.getMaxSize() != null) {
                    expectedConditions.add(new MaxSizeCondition(step.getMaxSize()));
                }
                if (step.getMaxDocs() != null) {
                    expectedConditions.add(new MaxDocsCondition(step.getMaxDocs()));
                }
                assertRolloverIndexRequest(request, alias, expectedConditions);
                listener.onResponse(new RolloverResponse(null, null, Collections.emptyMap(), request.isDryRun(), false, true, true));
                return null;
            }

        }).when(indicesClient).rolloverIndex(Mockito.any(), Mockito.any());

        SetOnce<Boolean> actionCompleted = new SetOnce<>();
        step.evaluateCondition(indexMetaData, new AsyncWaitStep.Listener() {

            @Override
            public void onResponse(boolean complete, ToXContentObject obj) {
                actionCompleted.set(complete);
            }

            @Override
            public void onFailure(Exception e) {
                throw new AssertionError("Unexpected method call", e);
            }
        });

        assertEquals(false, actionCompleted.get());

        Mockito.verify(client, Mockito.only()).admin();
        Mockito.verify(adminClient, Mockito.only()).indices();
        Mockito.verify(indicesClient, Mockito.only()).rolloverIndex(Mockito.any(), Mockito.any());
    }

    public void testPerformActionFailure() {
        String alias = randomAlphaOfLength(5);
        IndexMetaData indexMetaData = IndexMetaData.builder(randomAlphaOfLength(10))
            .putAlias(AliasMetaData.builder(alias))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        Exception exception = new RuntimeException();
        RolloverStep step = createRandomInstance();

        AdminClient adminClient = Mockito.mock(AdminClient.class);
        IndicesAdminClient indicesClient = Mockito.mock(IndicesAdminClient.class);

        Mockito.when(client.admin()).thenReturn(adminClient);
        Mockito.when(adminClient.indices()).thenReturn(indicesClient);
        Mockito.doAnswer(new Answer<Void>() {

            @Override
            public Void answer(InvocationOnMock invocation) throws Throwable {
                RolloverRequest request = (RolloverRequest) invocation.getArguments()[0];
                @SuppressWarnings("unchecked")
                ActionListener<RolloverResponse> listener = (ActionListener<RolloverResponse>) invocation.getArguments()[1];
                Set<Condition<?>> expectedConditions = new HashSet<>();
                if (step.getMaxAge() != null) {
                    expectedConditions.add(new MaxAgeCondition(step.getMaxAge()));
                }
                if (step.getMaxSize() != null) {
                    expectedConditions.add(new MaxSizeCondition(step.getMaxSize()));
                }
                if (step.getMaxDocs() != null) {
                    expectedConditions.add(new MaxDocsCondition(step.getMaxDocs()));
                }
                assertRolloverIndexRequest(request, alias, expectedConditions);
                listener.onFailure(exception);
                return null;
            }

        }).when(indicesClient).rolloverIndex(Mockito.any(), Mockito.any());

        SetOnce<Boolean> exceptionThrown = new SetOnce<>();
        step.evaluateCondition(indexMetaData, new AsyncWaitStep.Listener() {

            @Override
            public void onResponse(boolean complete, ToXContentObject obj) {
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
        IndexMetaData indexMetaData = IndexMetaData.builder(randomAlphaOfLength(10))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        RolloverStep step = createRandomInstance();

        SetOnce<Exception> exceptionThrown = new SetOnce<>();
        step.evaluateCondition(indexMetaData, new AsyncWaitStep.Listener() {
            @Override
            public void onResponse(boolean complete, ToXContentObject obj) {
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
            indexMetaData.getIndex().getName())));
    }

    public void testPerformActionAliasDoesNotPointToIndex() {
        String alias = randomAlphaOfLength(5);
        IndexMetaData indexMetaData = IndexMetaData.builder(randomAlphaOfLength(10))
            .settings(settings(Version.CURRENT).put(RolloverAction.LIFECYCLE_ROLLOVER_ALIAS, alias))
            .numberOfShards(randomIntBetween(1, 5)).numberOfReplicas(randomIntBetween(0, 5)).build();
        RolloverStep step = createRandomInstance();

        SetOnce<Exception> exceptionThrown = new SetOnce<>();
        step.evaluateCondition(indexMetaData, new AsyncWaitStep.Listener() {
            @Override
            public void onResponse(boolean complete, ToXContentObject obj) {
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
            indexMetaData.getIndex().getName())));

    }
}
