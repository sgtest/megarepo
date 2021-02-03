/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ilm;

import org.elasticsearch.common.io.stream.Writeable.Reader;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ilm.Step.StepKey;

import java.io.IOException;
import java.util.List;

public class DeleteActionTests extends AbstractActionTestCase<DeleteAction> {

    @Override
    protected DeleteAction doParseInstance(XContentParser parser) throws IOException {
        return DeleteAction.parse(parser);
    }

    @Override
    protected DeleteAction createTestInstance() {
        return new DeleteAction();
    }

    @Override
    protected Reader<DeleteAction> instanceReader() {
        return DeleteAction::new;
    }

    public void testToSteps() {
        String phase = randomAlphaOfLengthBetween(1, 10);
        StepKey nextStepKey = new StepKey(randomAlphaOfLengthBetween(1, 10), randomAlphaOfLengthBetween(1, 10),
            randomAlphaOfLengthBetween(1, 10));
        {
            DeleteAction action = new DeleteAction(true);
            List<Step> steps = action.toSteps(null, phase, nextStepKey);
            assertNotNull(steps);
            assertEquals(3, steps.size());
            StepKey expectedFirstStepKey = new StepKey(phase, DeleteAction.NAME, WaitForNoFollowersStep.NAME);
            StepKey expectedSecondStepKey = new StepKey(phase, DeleteAction.NAME, CleanupSnapshotStep.NAME);
            StepKey expectedThirdKey = new StepKey(phase, DeleteAction.NAME, DeleteStep.NAME);
            WaitForNoFollowersStep firstStep = (WaitForNoFollowersStep) steps.get(0);
            CleanupSnapshotStep secondStep = (CleanupSnapshotStep) steps.get(1);
            DeleteStep thirdStep = (DeleteStep) steps.get(2);
            assertEquals(expectedFirstStepKey, firstStep.getKey());
            assertEquals(expectedSecondStepKey, firstStep.getNextStepKey());
            assertEquals(expectedSecondStepKey, secondStep.getKey());
            assertEquals(expectedThirdKey, thirdStep.getKey());
            assertEquals(nextStepKey, thirdStep.getNextStepKey());
        }

        {
            DeleteAction actionKeepsSnapshot = new DeleteAction(false);
            List<Step> steps = actionKeepsSnapshot.toSteps(null, phase, nextStepKey);
            StepKey expectedFirstStepKey = new StepKey(phase, DeleteAction.NAME, WaitForNoFollowersStep.NAME);
            StepKey expectedSecondStepKey = new StepKey(phase, DeleteAction.NAME, DeleteStep.NAME);
            assertEquals(2, steps.size());
            assertNotNull(steps);
            WaitForNoFollowersStep firstStep = (WaitForNoFollowersStep) steps.get(0);
            DeleteStep secondStep = (DeleteStep) steps.get(1);
            assertEquals(expectedFirstStepKey, firstStep.getKey());
            assertEquals(expectedSecondStepKey, firstStep.getNextStepKey());
            assertEquals(nextStepKey, secondStep.getNextStepKey());
        }
    }
}
