/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.core.ilm;

import org.elasticsearch.client.Client;
import org.elasticsearch.cluster.health.ClusterHealthStatus;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.xpack.core.ilm.Step.StepKey;

import java.io.IOException;
import java.util.Arrays;
import java.util.List;

/**
 * Converts a CCR following index into a normal, standalone index, once the index is ready to be safely separated.
 *
 * "Readiness" is composed of two conditions:
 * 1) The index must have {@link LifecycleSettings#LIFECYCLE_INDEXING_COMPLETE} set to {@code true}, which is
 *      done automatically by {@link RolloverAction} (or manually).
 * 2) The index must be up to date with the leader, defined as the follower checkpoint being
 *      equal to the global checkpoint for all shards.
 */
public final class UnfollowAction implements LifecycleAction {

    public static final String NAME = "unfollow";
    public static final String CCR_METADATA_KEY = "ccr";
    static final String OPEN_FOLLOWER_INDEX_STEP_NAME = "open-follower-index";

    public UnfollowAction() {}

    @Override
    public List<Step> toSteps(Client client, String phase, StepKey nextStepKey) {
        StepKey indexingComplete = new StepKey(phase, NAME, WaitForIndexingCompleteStep.NAME);
        StepKey waitForFollowShardTasks = new StepKey(phase, NAME, WaitForFollowShardTasksStep.NAME);
        StepKey pauseFollowerIndex = new StepKey(phase, NAME, PauseFollowerIndexStep.NAME);
        StepKey closeFollowerIndex = new StepKey(phase, NAME, CloseFollowerIndexStep.NAME);
        StepKey unfollowFollowerIndex = new StepKey(phase, NAME, UnfollowFollowerIndexStep.NAME);
        // maintaining the `open-follower-index` here (as opposed to {@link OpenIndexStep#NAME}) for BWC reasons (in case any managed
        // index is in the `open-follower-index` step at upgrade time
        StepKey openFollowerIndex = new StepKey(phase, NAME, OPEN_FOLLOWER_INDEX_STEP_NAME);
        StepKey waitForYellowStep = new StepKey(phase, NAME, WaitForIndexColorStep.NAME);

        WaitForIndexingCompleteStep step1 = new WaitForIndexingCompleteStep(indexingComplete, waitForFollowShardTasks);
        WaitForFollowShardTasksStep step2 = new WaitForFollowShardTasksStep(waitForFollowShardTasks, pauseFollowerIndex, client);
        PauseFollowerIndexStep step3 = new PauseFollowerIndexStep(pauseFollowerIndex, closeFollowerIndex, client);
        CloseFollowerIndexStep step4 = new CloseFollowerIndexStep(closeFollowerIndex, unfollowFollowerIndex, client);
        UnfollowFollowerIndexStep step5 = new UnfollowFollowerIndexStep(unfollowFollowerIndex, openFollowerIndex, client);
        OpenIndexStep step6 = new OpenIndexStep(openFollowerIndex, waitForYellowStep, client);
        WaitForIndexColorStep step7 = new WaitForIndexColorStep(waitForYellowStep, nextStepKey, ClusterHealthStatus.YELLOW);
        return Arrays.asList(step1, step2, step3, step4, step5, step6, step7);
    }

    @Override
    public boolean isSafeAction() {
        // There are no settings to change, so therefor this action should be safe:
        return true;
    }

    @Override
    public String getWriteableName() {
        return NAME;
    }

    public UnfollowAction(StreamInput in) throws IOException {}

    @Override
    public void writeTo(StreamOutput out) throws IOException {}

    private static final ObjectParser<UnfollowAction, Void> PARSER = new ObjectParser<>(NAME, UnfollowAction::new);

    public static UnfollowAction parse(XContentParser parser) {
        return PARSER.apply(parser, null);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.endObject();
        return builder;
    }

    @Override
    public int hashCode() {
        return 36970;
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null) {
            return false;
        }
        if (obj.getClass() != getClass()) {
            return false;
        }
        return true;
    }

    @Override
    public String toString() {
        return Strings.toString(this);
    }
}
