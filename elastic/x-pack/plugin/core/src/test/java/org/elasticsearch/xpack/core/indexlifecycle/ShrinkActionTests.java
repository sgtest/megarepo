/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.indexlifecycle;

import org.elasticsearch.common.io.stream.Writeable.Reader;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.indexlifecycle.Step.StepKey;

import java.io.IOException;
import java.util.List;

import static org.hamcrest.Matchers.equalTo;

public class ShrinkActionTests extends AbstractSerializingTestCase<ShrinkAction> {

    @Override
    protected ShrinkAction doParseInstance(XContentParser parser) throws IOException {
        return ShrinkAction.parse(parser);
    }

    @Override
    protected ShrinkAction createTestInstance() {
        return new ShrinkAction(randomIntBetween(1, 100));
    }

    @Override
    protected ShrinkAction mutateInstance(ShrinkAction action) {
        return new ShrinkAction(action.getNumberOfShards() + randomIntBetween(1, 2));
    }

    @Override
    protected Reader<ShrinkAction> instanceReader() {
        return ShrinkAction::new;
    }

    public void testNonPositiveShardNumber() {
        Exception e = expectThrows(Exception.class, () -> new ShrinkAction(randomIntBetween(-100, 0)));
        assertThat(e.getMessage(), equalTo("[number_of_shards] must be greater than 0"));
    }

    public void testToSteps() {
        ShrinkAction action = createTestInstance();
        String phase = randomAlphaOfLengthBetween(1, 10);
        StepKey nextStepKey = new StepKey(randomAlphaOfLengthBetween(1, 10), randomAlphaOfLengthBetween(1, 10),
            randomAlphaOfLengthBetween(1, 10));
        List<Step> steps = action.toSteps(null, phase, nextStepKey);
        assertThat(steps.size(), equalTo(6));
        StepKey expectedFirstKey = new StepKey(phase, ShrinkAction.NAME, SetSingleNodeAllocateStep.NAME);
        StepKey expectedSecondKey = new StepKey(phase, ShrinkAction.NAME, AllocationRoutedStep.NAME);
        StepKey expectedThirdKey = new StepKey(phase, ShrinkAction.NAME, ShrinkStep.NAME);
        StepKey expectedFourthKey = new StepKey(phase, ShrinkAction.NAME, ShrunkShardsAllocatedStep.NAME);
        StepKey expectedFifthKey = new StepKey(phase, ShrinkAction.NAME, ShrinkSetAliasStep.NAME);
        StepKey expectedSixthKey = new StepKey(phase, ShrinkAction.NAME, ShrunkenIndexCheckStep.NAME);
        assertTrue(steps.get(0) instanceof SetSingleNodeAllocateStep);
        assertThat(steps.get(0).getKey(), equalTo(expectedFirstKey));
        assertThat(steps.get(0).getNextStepKey(), equalTo(expectedSecondKey));
        assertTrue(steps.get(1) instanceof AllocationRoutedStep);
        assertThat(steps.get(1).getKey(), equalTo(expectedSecondKey));
        assertThat(steps.get(1).getNextStepKey(), equalTo(expectedThirdKey));
        assertThat(((AllocationRoutedStep) steps.get(1)).getWaitOnAllShardCopies(), equalTo(false));
        assertTrue(steps.get(2) instanceof ShrinkStep);
        assertThat(steps.get(2).getKey(), equalTo(expectedThirdKey));
        assertThat(steps.get(2).getNextStepKey(), equalTo(expectedFourthKey));
        assertThat(((ShrinkStep) steps.get(2)).getNumberOfShards(), equalTo(action.getNumberOfShards()));
        assertThat(((ShrinkStep) steps.get(2)).getShrunkIndexPrefix(), equalTo(ShrinkAction.SHRUNKEN_INDEX_PREFIX));
        assertTrue(steps.get(3) instanceof ShrunkShardsAllocatedStep);
        assertThat(steps.get(3).getKey(), equalTo(expectedFourthKey));
        assertThat(steps.get(3).getNextStepKey(), equalTo(expectedFifthKey));
        assertThat(((ShrunkShardsAllocatedStep) steps.get(3)).getNumberOfShards(), equalTo(action.getNumberOfShards()));
        assertThat(((ShrunkShardsAllocatedStep) steps.get(3)).getShrunkIndexPrefix(), equalTo(ShrinkAction.SHRUNKEN_INDEX_PREFIX));
        assertTrue(steps.get(4) instanceof ShrinkSetAliasStep);
        assertThat(steps.get(4).getKey(), equalTo(expectedFifthKey));
        assertThat(steps.get(4).getNextStepKey(), equalTo(expectedSixthKey));
        assertThat(((ShrinkSetAliasStep) steps.get(4)).getShrunkIndexPrefix(), equalTo(ShrinkAction.SHRUNKEN_INDEX_PREFIX));
        assertTrue(steps.get(5) instanceof ShrunkenIndexCheckStep);
        assertThat(steps.get(5).getKey(), equalTo(expectedSixthKey));
        assertThat(steps.get(5).getNextStepKey(), equalTo(nextStepKey));
        assertThat(((ShrunkenIndexCheckStep) steps.get(5)).getShrunkIndexPrefix(), equalTo(ShrinkAction.SHRUNKEN_INDEX_PREFIX));
    }
}
